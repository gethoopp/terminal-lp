use serde::Deserialize;

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
    pub volume: Option<VolumeData>,
    pub liquidity: Option<LiquidityData>,
    #[serde(rename = "fdv")]
    pub fdv: Option<f64>,
    #[serde(rename = "pairCreatedAt")]
    pub pair_created_at: Option<u64>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenInfo {
    pub name: Option<String>,
    pub symbol: Option<String>,
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

/// Fetch top volatile pairs from DexScreener for a given search query (e.g. token address or name)
pub async fn search_pairs(query: &str) -> Result<Vec<PairData>, String> {
    let url = format!("{}/search?q={}", DEXSCREENER_API, query);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to build client: {}", e))?;

    let resp = client
        .get(&url)
        .header("User-Agent", "volatile-scanner/0.1")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("API returned status: {}", resp.status()));
    }

    let body = resp.text().await.map_err(|e| format!("Read body: {}", e))?;
    let data: DexResponse =
        serde_json::from_str(&body).map_err(|e| format!("Parse error: {}", e))?;

    Ok(data.pairs.unwrap_or_default())
}

/// Fetch trending pairs on Solana (by searching common base tokens)
pub async fn fetch_volatile_pairs() -> Result<Vec<PairData>, String> {
    // Search for Solana pairs - broad search to catch volatile tokens
    let mut all_pairs: Vec<PairData> = Vec::new();

    // Multiple queries to catch different volatile segments
    let queries = vec!["SOL", "USDC", "BONK", "WIF", "JUP", "RAY"];

    for query in queries {
        match search_pairs(query).await {
            Ok(pairs) => {
                for pair in pairs {
                    if pair.chain_id == "solana" {
                        all_pairs.push(pair);
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning: search '{}' failed: {}", query, e);
            }
        }
    }

    // Deduplicate by pair address (using base token symbol as key)
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

    Ok(all_pairs)
}
