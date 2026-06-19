use crate::config;
use serde::Deserialize;
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
    pub boosts: Option<BoostsData>,
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

#[derive(Debug, Clone, Deserialize)]
pub struct BoostsData {
    pub active: Option<u32>,
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

// ── Meteora Pool Discovery API ──────────────────────────────────────────────

const METEORA_POOL_DISCOVERY_BASE: &str = "https://pool-discovery-api.datapi.meteora.ag";

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MeteoraTokenInfo {
    pub symbol: Option<String>,
    pub address: Option<String>,
    pub organic_score: Option<f64>,
    pub market_cap: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MeteoraDlmmParams {
    pub bin_step: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MeteoraRawPool {
    pub pool_address: Option<String>,
    pub name: Option<String>,
    pub token_x: Option<MeteoraTokenInfo>,
    pub token_y: Option<MeteoraTokenInfo>,
    pub dlmm_params: Option<MeteoraDlmmParams>,
    pub fee_pct: Option<f64>,
    pub tvl: Option<f64>,
    pub active_tvl: Option<f64>,
    pub fee: Option<f64>,
    pub volume: Option<f64>,
    pub fee_active_tvl_ratio: Option<f64>,
    pub volatility: Option<f64>,
    pub base_token_holders: Option<u32>,
    pub pool_price_change_pct: Option<f64>,
}

/// Condensed pool data ready for display and scoring.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MeteoraPairData {
    pub pool_address: String,
    pub name: String,
    pub base_symbol: String,
    pub base_mint: String,
    pub base_organic: f64,
    pub quote_symbol: String,
    pub bin_step: Option<u32>,
    pub fee_pct: f64,
    pub tvl: f64,
    pub active_tvl: f64,
    pub fee_window: f64,
    pub volume_window: f64,
    pub fee_active_tvl_ratio: f64,
    pub volatility: f64,
    pub holders: u32,
    pub mcap: f64,
    pub organic_score: f64,
    pub price_change_pct: f64,
}

impl MeteoraRawPool {
    fn is_usable(&self) -> bool {
        self.volatility.map(|v| v > 0.0 && v.is_finite()).unwrap_or(false)
    }

    fn condense(&self) -> Option<MeteoraPairData> {
        if !self.is_usable() {
            return None;
        }
        Some(MeteoraPairData {
            pool_address: self.pool_address.clone().unwrap_or_default(),
            name: self.name.clone().unwrap_or_default(),
            base_symbol: self.token_x.as_ref().and_then(|t| t.symbol.clone()).unwrap_or_default(),
            base_mint: self.token_x.as_ref().and_then(|t| t.address.clone()).unwrap_or_default(),
            base_organic: self.token_x.as_ref().and_then(|t| t.organic_score).unwrap_or(0.0),
            quote_symbol: self.token_y.as_ref().and_then(|t| t.symbol.clone()).unwrap_or_default(),
            bin_step: self.dlmm_params.as_ref().and_then(|d| d.bin_step),
            fee_pct: self.fee_pct.unwrap_or(0.0),
            tvl: self.tvl.unwrap_or(0.0),
            active_tvl: self.active_tvl.unwrap_or(0.0),
            fee_window: self.fee.unwrap_or(0.0),
            volume_window: self.volume.unwrap_or(0.0),
            fee_active_tvl_ratio: self.fee_active_tvl_ratio.unwrap_or(0.0),
            volatility: self.volatility.unwrap_or(0.0),
            holders: self.base_token_holders.unwrap_or(0),
            mcap: self.token_x.as_ref().and_then(|t| t.market_cap).unwrap_or(0.0),
            organic_score: self.token_x.as_ref().and_then(|t| t.organic_score).unwrap_or(0.0),
            price_change_pct: self.pool_price_change_pct.unwrap_or(0.0),
        })
    }
}

impl MeteoraPairData {
    /// Meridian scoring: feeTvl * 1000 + organic * 10 + volume / 100 + holders / 100
    pub fn score(&self) -> f64 {
        self.fee_active_tvl_ratio * 1000.0
            + self.organic_score * 10.0
            + self.volume_window / 100.0
            + self.holders as f64 / 100.0
    }
}

pub struct MeteoraFetchResult {
    pub pools: Vec<MeteoraPairData>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MeteoraApiResponse {
    data: Option<Vec<MeteoraRawPool>>,
    total: Option<u64>,
}

/// Fetch DLMM pools from the Meteora Pool Discovery API using Meridian's screening thresholds.
pub async fn fetch_meteora_pools(client: &reqwest::Client) -> Result<MeteoraFetchResult, String> {
    let cfg = &config::active_config().meteora;
    let mut warnings: Vec<String> = Vec::new();

    let filters = format!(
        "base_token_has_critical_warnings=false\
        &&quote_token_has_critical_warnings=false\
        &&pool_type=dlmm\
        &&base_token_market_cap>={min_mcap}\
        &&base_token_market_cap<={max_mcap}\
        &&base_token_holders>={min_holders}\
        &&volume>={min_volume}\
        &&tvl>={min_tvl}\
        &&tvl<={max_tvl}\
        &&dlmm_bin_step>={min_bin_step}\
        &&dlmm_bin_step<={max_bin_step}\
        &&fee_active_tvl_ratio>={min_fee_tvl}\
        &&base_token_organic_score>={min_organic}",
        min_mcap = cfg.min_mcap,
        max_mcap = cfg.max_mcap,
        min_holders = cfg.min_holders,
        min_volume = cfg.min_volume,
        min_tvl = cfg.min_tvl,
        max_tvl = cfg.max_tvl,
        min_bin_step = cfg.min_bin_step,
        max_bin_step = cfg.max_bin_step,
        min_fee_tvl = cfg.min_fee_active_tvl_ratio,
        min_organic = cfg.min_organic,
    );

    // Percent-encode the filter string (mirrors JS encodeURIComponent)
    let encoded: String = filters
        .chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '!' | '~' | '*' | '\'' | '(' | ')' => {
                vec![c]
            }
            c => format!("%{:02X}", c as u32).chars().collect(),
        })
        .collect();

    let url = format!(
        "{}/pools?page_size={}&filter_by={}&timeframe={}&category={}",
        METEORA_POOL_DISCOVERY_BASE,
        cfg.page_size,
        encoded,
        cfg.timeframe,
        cfg.category,
    );

    let resp = client
        .get(&url)
        .header("User-Agent", "volatile-scanner/0.1")
        .send()
        .await
        .map_err(|e| format!("Meteora Pool Discovery API request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        return Err(format!("Meteora Pool Discovery API returned status: {status}"));
    }

    let body = resp.text().await.map_err(|e| format!("Meteora API read body: {e}"))?;
    let data: MeteoraApiResponse = serde_json::from_str(&body)
        .map_err(|e| format!("Meteora API parse error: {e}"))?;

    let raw_pools = data.data.unwrap_or_default();
    let total = data.total.unwrap_or(0);

    let mut pools: Vec<MeteoraPairData> = raw_pools
        .iter()
        .filter_map(|p| p.condense())
        .collect();

    // Sort by Meridian score descending
    pools.sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(std::cmp::Ordering::Equal));

    warnings.push(format!(
        "Meteora Pool Discovery: {} pools screened (total on API: {})",
        pools.len(),
        total
    ));

    Ok(MeteoraFetchResult { pools, warnings })
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

#[allow(dead_code)]
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
        boosts: None,
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

    let mut all_addresses: Vec<String> = Vec::new();
    let mut warnings = Vec::new();

    // 1. Fetch from token-boosts endpoint (trending tokens)
    let boosts_url = "https://api.dexscreener.com/token-boosts/top/v1";
    if let Ok(resp) = client
        .get(boosts_url)
        .header("User-Agent", "volatile-scanner/0.1")
        .send()
        .await
    {
        if let Ok(body) = resp.text().await {
            #[derive(serde::Deserialize)]
            struct TokenItem {
                #[serde(rename = "chainId")]
                chain_id: String,
                #[serde(rename = "tokenAddress")]
                token_address: String,
            }
            let boosts: Vec<TokenItem> = serde_json::from_str(&body).unwrap_or_default();
            for item in boosts {
                if item.chain_id == "solana" && !all_addresses.contains(&item.token_address) {
                    all_addresses.push(item.token_address);
                }
            }
        }
    }

    // 2. Fetch from token-boosts latest
    let boosts_latest_url = "https://api.dexscreener.com/token-boosts/latest/v1";
    if let Ok(resp) = client
        .get(boosts_latest_url)
        .header("User-Agent", "volatile-scanner/0.1")
        .send()
        .await
    {
        if let Ok(body) = resp.text().await {
            #[derive(serde::Deserialize)]
            struct TokenItem {
                #[serde(rename = "chainId")]
                chain_id: String,
                #[serde(rename = "tokenAddress")]
                token_address: String,
            }
            let boosts: Vec<TokenItem> = serde_json::from_str(&body).unwrap_or_default();
            for item in boosts {
                if item.chain_id == "solana" && !all_addresses.contains(&item.token_address) {
                    all_addresses.push(item.token_address);
                }
            }
        }
    }

    // 3. Also search for established Solana tokens
    let queries = api_cfg.queries.clone();
    for (idx, query) in queries.iter().enumerate() {
        match search_pairs_with_retry(client, query, api_cfg.max_retries).await {
            Ok(pairs) => {
                debug!(query = %query, pairs = pairs.len(), "dexscreener query success");
                for pair in pairs {
                    if pair.chain_id == "solana" {
                        if let Some(addr) = &pair.base_token.address {
                            if !all_addresses.contains(addr) {
                                all_addresses.push(addr.clone());
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

    if all_addresses.is_empty() && api_cfg.jupiter_enabled {
        match fetch_from_jupiter_fallback(client).await {
            Ok(jupiter_pairs) => {
                warnings.push("DexScreener unavailable, using Jupiter fallback quote".to_string());
                return Ok(FetchResult { pairs: jupiter_pairs, warnings });
            }
            Err(e) => {
                warnings.push(format!("Jupiter fallback failed: {e}"));
            }
        }
    }

    // Take up to 80 unique addresses
    all_addresses.truncate(80);

    // 4. Batch fetch pairs for those tokens
    let chunks: Vec<String> = all_addresses
        .chunks(10)
        .map(|c| c.join(","))
        .collect();

    let mut all_pairs: Vec<PairData> = Vec::new();
    let mut seen_pairs: std::collections::HashSet<String> = std::collections::HashSet::new();

    for chunk in &chunks {
        let url = format!("https://api.dexscreener.com/latest/dex/tokens/{chunk}");
        match client
            .get(&url)
            .header("User-Agent", "volatile-scanner/0.1")
            .send()
            .await
        {
            Ok(resp) => {
                if let Ok(body) = resp.text().await {
                    if let Ok(data) = serde_json::from_str::<DexResponse>(&body) {
                        for pair in data.pairs.unwrap_or_default() {
                            if pair.chain_id == "solana" {
                                let key = pair.pair_address.clone().unwrap_or_default();
                                if !key.is_empty() && !seen_pairs.contains(&key) {
                                    seen_pairs.insert(key);
                                    all_pairs.push(pair);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => warnings.push(format!("token fetch failed: {e}")),
        }
        sleep(Duration::from_millis(api_cfg.query_delay_ms)).await;
    }

    // Sort by volume
    all_pairs.sort_by(|a, b| {
        let vol_a = a.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
        let vol_b = b.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
        vol_b.partial_cmp(&vol_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    all_pairs.truncate(api_cfg.top_pairs_final);

    if !warnings.is_empty() {
        warnings.insert(0, format!("Fetched {} Solana pairs from DexScreener", all_pairs.len()));
    }

    Ok(FetchResult { pairs: all_pairs, warnings })
}

/// Fetch degen pairs: combines DexScreener token-boosts and token-profiles endpoints
/// to find recently boosted/profiled Solana tokens, then fetches their pair data.
/// This surfaces new/trending tokens that normal keyword queries miss.
pub async fn fetch_degen_pairs(client: &reqwest::Client) -> Result<FetchResult, String> {
    let api_cfg = &config::active_config().api;

    #[derive(serde::Deserialize)]
    struct TokenItem {
        #[serde(rename = "chainId")]
        chain_id: String,
        #[serde(rename = "tokenAddress")]
        token_address: String,
    }

    let mut all_addresses: Vec<String> = Vec::new();
    let mut warnings = Vec::new();

    // 1. Fetch from token-boosts endpoint (paid promotions)
    let boosts_url = "https://api.dexscreener.com/token-boosts/latest/v1";
    if let Ok(resp) = client
        .get(boosts_url)
        .header("User-Agent", "volatile-scanner/0.1")
        .send()
        .await
    {
        if let Ok(body) = resp.text().await {
            let boosts: Vec<TokenItem> = serde_json::from_str(&body).unwrap_or_default();
            for item in boosts {
                if item.chain_id == "solana" && !all_addresses.contains(&item.token_address) {
                    all_addresses.push(item.token_address);
                }
            }
        }
    }

    // 2. Fetch from token-profiles endpoint (tokens with custom profiles)
    let profiles_url = "https://api.dexscreener.com/token-profiles/latest/v1";
    if let Ok(resp) = client
        .get(profiles_url)
        .header("User-Agent", "volatile-scanner/0.1")
        .send()
        .await
    {
        if let Ok(body) = resp.text().await {
            let profiles: Vec<TokenItem> = serde_json::from_str(&body).unwrap_or_default();
            for item in profiles {
                if item.chain_id == "solana" && !all_addresses.contains(&item.token_address) {
                    all_addresses.push(item.token_address);
                }
            }
        }
    }

    // 3. Also fetch trending pairs from search for popular new tokens
    let trending_queries = ["pump", "meme", "pepe", "doge", "cat", "ai"];
    for query in trending_queries {
        let search_url = format!("https://api.dexscreener.com/latest/dex/search?q={query}");
        if let Ok(resp) = client
            .get(&search_url)
            .header("User-Agent", "volatile-scanner/0.1")
            .send()
            .await
        {
            if let Ok(body) = resp.text().await {
                if let Ok(data) = serde_json::from_str::<DexResponse>(&body) {
                    for pair in data.pairs.unwrap_or_default() {
                        if pair.chain_id == "solana" {
                            if let Some(addr) = &pair.base_token.address {
                                if !all_addresses.contains(addr) {
                                    all_addresses.push(addr.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        sleep(Duration::from_millis(api_cfg.query_delay_ms)).await;
    }

    // 4. Fetch from trending metas (new endpoint) — surfaces trending sectors on Solana
    let metas_url = "https://api.dexscreener.com/metas/trending/v1";
    if let Ok(resp) = client
        .get(metas_url)
        .header("User-Agent", "volatile-scanner/0.1")
        .send()
        .await
    {
        if let Ok(body) = resp.text().await {
            #[derive(serde::Deserialize)]
            struct MetaItem {
                slug: Option<String>,
            }
            let metas: Vec<MetaItem> = serde_json::from_str(&body).unwrap_or_default();
            // Fetch pairs for top 3 trending metas
            for meta in metas.iter().take(3) {
                if let Some(slug) = &meta.slug {
                    let meta_url = format!("https://api.dexscreener.com/metas/meta/v1/{slug}");
                    if let Ok(r) = client.get(&meta_url).header("User-Agent", "volatile-scanner/0.1").send().await {
                        if let Ok(b) = r.text().await {
                            #[derive(serde::Deserialize)]
                            struct MetaDetail { pairs: Option<Vec<MetaPair>> }
                            #[derive(serde::Deserialize)]
                            struct MetaPair {
                                #[serde(rename = "chainId")] chain_id: Option<String>,
                                #[serde(rename = "baseToken")] base_token: Option<MetaToken>,
                            }
                            #[derive(serde::Deserialize)]
                            struct MetaToken { address: Option<String> }
                            if let Ok(detail) = serde_json::from_str::<MetaDetail>(&b) {
                                for pair in detail.pairs.unwrap_or_default() {
                                    if pair.chain_id.as_deref() == Some("solana") {
                                        if let Some(addr) = pair.base_token.and_then(|t| t.address) {
                                            if !all_addresses.contains(&addr) {
                                                all_addresses.push(addr);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    sleep(Duration::from_millis(api_cfg.query_delay_ms)).await;
                }
            }
        }
    }

    // 5. Fetch recently updated token profiles (new endpoint)
    let recent_url = "https://api.dexscreener.com/token-profiles/recent-updates/v1";
    if let Ok(resp) = client
        .get(recent_url)
        .header("User-Agent", "volatile-scanner/0.1")
        .send()
        .await
    {
        if let Ok(body) = resp.text().await {
            #[derive(serde::Deserialize)]
            struct ProfileItem {
                #[serde(rename = "chainId")] chain_id: Option<String>,
                #[serde(rename = "tokenAddress")] token_address: Option<String>,
            }
            let profiles: Vec<ProfileItem> = serde_json::from_str(&body).unwrap_or_default();
            for item in profiles {
                if item.chain_id.as_deref() == Some("solana") {
                    if let Some(addr) = item.token_address {
                        if !all_addresses.contains(&addr) {
                            all_addresses.push(addr);
                        }
                    }
                }
            }
        }
    }

    if all_addresses.is_empty() {
        return Ok(FetchResult { pairs: vec![], warnings: vec!["No degen tokens found".to_string()] });
    }

    // Take up to 50 unique addresses
    all_addresses.truncate(50);

    // 4. Batch fetch pairs for those tokens
    let chunks: Vec<String> = all_addresses
        .chunks(10)
        .map(|c| c.join(","))
        .collect();

    let mut all_pairs: Vec<PairData> = Vec::new();
    let mut seen_pairs: std::collections::HashSet<String> = std::collections::HashSet::new();

    for chunk in &chunks {
        let url = format!("https://api.dexscreener.com/latest/dex/tokens/{chunk}");
        match client
            .get(&url)
            .header("User-Agent", "volatile-scanner/0.1")
            .send()
            .await
        {
            Ok(resp) => {
                if let Ok(body) = resp.text().await {
                    if let Ok(data) = serde_json::from_str::<DexResponse>(&body) {
                        for pair in data.pairs.unwrap_or_default() {
                            if pair.chain_id == "solana" {
                                let key = pair.pair_address.clone().unwrap_or_default();
                                if !key.is_empty() && !seen_pairs.contains(&key) {
                                    seen_pairs.insert(key);
                                    all_pairs.push(pair);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => warnings.push(format!("token fetch failed: {e}")),
        }
        sleep(Duration::from_millis(api_cfg.query_delay_ms)).await;
    }

    if !warnings.is_empty() {
        warnings.insert(0, format!("Fetched {} unique degen pairs", all_pairs.len()));
    }

    Ok(FetchResult { pairs: all_pairs, warnings })
}
