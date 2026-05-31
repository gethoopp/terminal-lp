use crate::config;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

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

#[derive(Debug, Deserialize)]
struct JupiterQuoteResponse {
    #[serde(rename = "outAmount")]
    out_amount: String,
    #[serde(rename = "priceImpactPct")]
    price_impact_pct: Option<String>,
}

pub struct FetchResult {
    pub pairs: Vec<PairData>,
    pub warnings: Vec<String>,
}

pub fn build_http_client() -> Result<reqwest::Client, String> {
    let timeout_secs = config::active_config().api.http_timeout_secs.max(1);
    reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))
}

async fn search_pairs(client: &reqwest::Client, query: &str) -> Result<Vec<PairData>, String> {
    let api_cfg = &config::active_config().api;
    let url = format!(
        "{}/search?q={query}",
        api_cfg.dexscreener_base_url.trim_end_matches('/')
    );

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

fn jupiter_fallback_pair(quote: &JupiterQuoteResponse) -> Option<PairData> {
    let api_cfg = &config::active_config().api;
    let in_amount =
        api_cfg.jupiter_amount as f64 / 10f64.powi(api_cfg.jupiter_input_decimals as i32);
    let out_amount_raw = quote.out_amount.parse::<f64>().ok()?;
    let out_amount = out_amount_raw / 10f64.powi(api_cfg.jupiter_output_decimals as i32);
    if in_amount <= 0.0 || out_amount <= 0.0 {
        return None;
    }

    let implied_price = out_amount / in_amount;
    let price_impact = quote
        .price_impact_pct
        .as_deref()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);

    Some(PairData {
        chain_id: "solana".to_string(),
        dex_id: "jupiter".to_string(),
        base_token: TokenInfo {
            name: Some("Solana".to_string()),
            symbol: Some("SOL".to_string()),
            address: Some(api_cfg.jupiter_input_mint.clone()),
        },
        quote_token: TokenInfo {
            name: Some("USD Coin".to_string()),
            symbol: Some("USDC".to_string()),
            address: Some(api_cfg.jupiter_output_mint.clone()),
        },
        price_usd: Some(format!("{implied_price:.8}")),
        price_change: Some(PriceChange {
            m5: None,
            h1: None,
            h6: None,
            h24: Some(-price_impact),
        }),
        txns: None,
        volume: None,
        liquidity: None,
        market_cap: None,
        fdv: None,
        pair_created_at: None,
        pair_address: Some(format!(
            "jupiter:{}:{}",
            api_cfg.jupiter_input_mint, api_cfg.jupiter_output_mint
        )),
        url: Some("https://jup.ag/".to_string()),
    })
}

async fn fetch_from_jupiter_fallback(client: &reqwest::Client) -> Result<Vec<PairData>, String> {
    let api_cfg = &config::active_config().api;
    let mut errors = Vec::new();

    for url in &api_cfg.jupiter_quote_urls {
        let mut request = client.get(url).query(&[
            ("inputMint", api_cfg.jupiter_input_mint.as_str()),
            ("outputMint", api_cfg.jupiter_output_mint.as_str()),
            ("amount", &api_cfg.jupiter_amount.to_string()),
            ("slippageBps", &api_cfg.jupiter_slippage_bps.to_string()),
        ]);

        if !api_cfg.jupiter_api_key.is_empty() {
            request = request.header("x-api-key", api_cfg.jupiter_api_key.as_str());
        }

        let response = match request.send().await {
            Ok(resp) => resp,
            Err(err) => {
                errors.push(format!("{url} request failed: {err}"));
                continue;
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            errors.push(format!("{url} returned {status}: {body}"));
            continue;
        }

        let quote = match response.json::<JupiterQuoteResponse>().await {
            Ok(parsed) => parsed,
            Err(err) => {
                errors.push(format!("{url} parse failed: {err}"));
                continue;
            }
        };

        if let Some(pair) = jupiter_fallback_pair(&quote) {
            return Ok(vec![pair]);
        }

        errors.push(format!("{url} parsed but produced invalid quote payload"));
    }

    Err(errors.join(" | "))
}

pub async fn fetch_volatile_pairs(client: &reqwest::Client) -> Result<FetchResult, String> {
    let api_cfg = &config::active_config().api;

    let mut best_by_pair: HashMap<String, PairData> = HashMap::new();
    let mut warnings = Vec::new();
    let queries = api_cfg.queries.clone();

    for (idx, query) in queries.iter().enumerate() {
        match search_pairs_with_retry(client, query, api_cfg.max_retries).await {
            Ok(pairs) => {
                debug!(query = %query, pairs = pairs.len(), "dexscreener query success");
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
                warn!(query = %query, error = %e, "dexscreener query failed");
                warnings.push(format!("search '{query}' failed: {e}"));
            }
        }

        if idx + 1 < queries.len() {
            sleep(Duration::from_millis(api_cfg.query_delay_ms)).await;
        }
    }

    let mut all_pairs: Vec<PairData> = best_by_pair.into_values().collect();

    all_pairs.sort_by(|a, b| {
        let vol_a = a.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
        let vol_b = b.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
        vol_b
            .partial_cmp(&vol_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    all_pairs.truncate(api_cfg.top_pairs_by_volume);
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
    all_pairs.truncate(api_cfg.top_pairs_final);

    if all_pairs.is_empty() && api_cfg.jupiter_enabled {
        match fetch_from_jupiter_fallback(client).await {
            Ok(jupiter_pairs) => {
                warnings.push("DexScreener unavailable, using Jupiter fallback quote".to_string());
                all_pairs = jupiter_pairs;
            }
            Err(err) => {
                warnings.push(format!("Jupiter fallback failed: {err}"));
            }
        }
    }

    Ok(FetchResult {
        pairs: all_pairs,
        warnings,
    })
}
