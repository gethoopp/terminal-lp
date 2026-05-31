use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

const DEXSCREENER_API: &str = "https://api.dexscreener.com/latest/dex";

#[derive(Debug, Clone, Deserialize)]
pub struct DexResponse {
    pub pairs: Option<Vec<PairData>>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PairData {
    #[serde(rename = "chainId")]
    pub chain_id: String,
    #[serde(rename = "dexId")]
    pub dex_id: String,
    #[serde(rename = "baseToken")]
    pub base_token: TokenInfo,
    #[serde(rename = "quoteToken")]
    pub quote_token: TokenInfo,
    #[serde(rename = "priceUsd")]
    pub price_usd: Option<String>,
    #[serde(rename = "priceChange")]
    pub price_change: Option<PriceChange>,
    pub txns: Option<TxnsData>,
    pub volume: Option<VolumeData>,
    pub liquidity: Option<LiquidityData>,
    #[serde(rename = "marketCap")]
    pub market_cap: Option<f64>,
    #[serde(rename = "fdv")]
    pub fdv: Option<f64>,
    #[serde(rename = "pairCreatedAt")]
    pub pair_created_at: Option<u64>,
    #[serde(rename = "pairAddress")]
    pub pair_address: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TxnsData {
    pub m5: Option<TxnSideCount>,
    pub h1: Option<TxnSideCount>,
    pub h24: Option<TxnSideCount>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TxnSideCount {
    pub buys: Option<f64>,
    pub sells: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenInfo {
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub address: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PriceChange {
    pub m5: Option<f64>,
    pub h1: Option<f64>,
    pub h6: Option<f64>,
    pub h24: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct VolumeData {
    pub h24: Option<f64>,
    pub h6: Option<f64>,
    pub h1: Option<f64>,
    pub m5: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LiquidityData {
    pub usd: Option<f64>,
}

pub struct FetchResult {
    pub pairs: Vec<PairData>,
    pub warnings: Vec<String>,
}

pub fn build_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))
}

/// Fetch top volatile pairs from DexScreener for a given search query (e.g. token address or name)
async fn search_pairs(client: &reqwest::Client, query: &str) -> Result<Vec<PairData>, String> {
    let url = format!("{DEXSCREENER_API}/search?q={query}");

    let resp = client
        .get(&url)
        .header("User-Agent", "volatile-scanner/0.1")
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !resp.status().is_success() {
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err("API rate limit hit (429)".to_string());
        }
        let status = resp.status();
        return Err(format!("API returned status: {status}"));
    }

    let body = resp.text().await.map_err(|e| format!("Read body: {e}"))?;
    let data: DexResponse = serde_json::from_str(&body).map_err(|e| format!("Parse error: {e}"))?;

    Ok(data.pairs.unwrap_or_default())
}

async fn search_pairs_with_retry(
    client: &reqwest::Client,
    query: &str,
    max_retries: usize,
) -> Result<Vec<PairData>, String> {
    let mut backoff = Duration::from_millis(300);
    let mut last_error = String::from("Unknown error");

    for attempt in 0..=max_retries {
        match search_pairs(client, query).await {
            Ok(pairs) => return Ok(pairs),
            Err(err) => {
                last_error = err;
                if attempt < max_retries {
                    sleep(backoff).await;
                    backoff = backoff.saturating_mul(2);
                }
            }
        }
    }

    Err(last_error)
}

fn pair_identity(pair: &PairData) -> String {
    pair.pair_address
        .clone()
        .or_else(|| pair.url.clone())
        .unwrap_or_else(|| {
            format!(
                "{}:{}:{}:{}",
                pair.chain_id,
                pair.dex_id,
                pair.base_token.symbol.as_deref().unwrap_or(""),
                pair.quote_token.symbol.as_deref().unwrap_or("")
            )
        })
}

/// Fetch trending pairs on Solana (by searching common base tokens)
pub async fn fetch_volatile_pairs(client: &reqwest::Client) -> Result<FetchResult, String> {
    // Search for Solana pairs - broad search to catch volatile tokens
    let mut best_by_pair: HashMap<String, PairData> = HashMap::new();
    let mut warnings = Vec::new();

    // Multiple queries to catch different volatile segments
    let queries = ["SOL", "USDC", "BONK", "WIF", "JUP", "RAY"];

    for (idx, query) in queries.iter().enumerate() {
        match search_pairs_with_retry(client, query, 1).await {
            Ok(pairs) => {
                for pair in pairs {
                    if pair.chain_id == "solana" {
                        let key = pair_identity(&pair);
                        let incoming_vol = pair.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);

                        match best_by_pair.get(&key) {
                            Some(existing) => {
                                let existing_vol =
                                    existing.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
                                if incoming_vol > existing_vol {
                                    best_by_pair.insert(key, pair);
                                }
                            }
                            None => {
                                best_by_pair.insert(key, pair);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warnings.push(format!("search '{query}' failed: {e}"));
            }
        }

        // Throttle requests to reduce API rate-limit risk.
        if idx + 1 < queries.len() {
            sleep(Duration::from_millis(250)).await;
        }
    }

    let mut all_pairs: Vec<PairData> = best_by_pair.into_values().collect();

    // Keep top 100 by volume first.
    all_pairs.sort_by(|a, b| {
        let vol_a = a.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
        let vol_b = b.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
        vol_b
            .partial_cmp(&vol_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Keep top 100 by volume, then sort by absolute price change
    all_pairs.truncate(100);
    all_pairs.sort_by(|a, b| {
        let change_a = a
            .price_change
            .as_ref()
            .and_then(|pc| pc.h24)
            .map(|v| v.abs())
            .unwrap_or(0.0);
        let change_b = b
            .price_change
            .as_ref()
            .and_then(|pc| pc.h24)
            .map(|v| v.abs())
            .unwrap_or(0.0);
        change_b
            .partial_cmp(&change_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(FetchResult {
        pairs: all_pairs,
        warnings,
    })
}
