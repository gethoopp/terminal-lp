use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefiLlamaYieldResponse {
    pub data: Vec<YieldPool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct YieldPool {
    pub chain: Option<String>,
    pub project: Option<String>,
    pub symbol: Option<String>,
    #[serde(rename = "tvlUsd")]
    pub tvl_usd: Option<f64>,
    #[serde(rename = "apyBase")]
    pub apy_base: Option<f64>,
    #[serde(rename = "apyReward")]
    pub apy_reward: Option<f64>,
    pub apy: Option<f64>,
    pub pool: Option<String>,
    #[serde(rename = "ilRisk")]
    pub il_risk: Option<String>,
    pub exposure: Option<String>,
    pub stablecoin: Option<bool>,
    #[serde(rename = "poolMeta")]
    pub pool_meta: Option<String>,
    #[serde(rename = "underlyingTokens")]
    pub underlying_tokens: Option<Vec<String>>,
    #[serde(rename = "apyBase7d")]
    pub apy_base_7d: Option<f64>,
    #[serde(rename = "apyMean30d")]
    pub apy_mean_30d: Option<f64>,
    #[serde(rename = "volumeUsd1d")]
    pub volume_usd_1d: Option<f64>,
    #[serde(rename = "volumeUsd7d")]
    pub volume_usd_7d: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldInfo {
    pub symbol: String,
    pub project: String,
    pub dex_name: DexName,
    pub apy: Option<f64>,
    pub apy_base: Option<f64>,
    pub apy_reward: Option<f64>,
    pub tvl_usd: f64,
    pub il_risk: bool,
    pub volume_24h: Option<f64>,
    pub apy_mean_30d: Option<f64>,
    pub underlying_tokens: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DexName {
    Meteora,
    Raydium,
    Orca,
    Kamino,
    PumpSwap,
    Other,
}

impl DexName {
    pub fn label(&self) -> &'static str {
        match self {
            DexName::Meteora => "Meteora",
            DexName::Raydium => "Raydium",
            DexName::Orca => "Orca",
            DexName::Kamino => "Kamino",
            DexName::PumpSwap => "PumpSwap",
            DexName::Other => "Other",
        }
    }

    pub fn from_project(project: &str) -> Self {
        let p = project.to_ascii_lowercase();
        if p.contains("meteora") {
            DexName::Meteora
        } else if p.contains("raydium") {
            DexName::Raydium
        } else if p.contains("orca") {
            DexName::Orca
        } else if p.contains("kamino") {
            DexName::Kamino
        } else if p.contains("pump") {
            DexName::PumpSwap
        } else {
            DexName::Other
        }
    }
}

impl YieldInfo {
    pub fn display_apy(&self) -> String {
        match self.apy {
            Some(apy) => format!("{apy:.2}%"),
            None => "N/A".to_string(),
        }
    }
}

pub async fn fetch_yields(
    client: &reqwest::Client,
) -> Result<HashMap<String, Vec<YieldInfo>>, String> {
    let url = "https://yields.llama.fi/pools";

    debug!("Fetching yield data from DefiLlama");

    let resp = client
        .get(url)
        .header("User-Agent", "volatile-scanner/0.1")
        .send()
        .await
        .map_err(|e| format!("DefiLlama request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("DefiLlama returned status: {}", resp.status()));
    }

    let body = resp.text().await.map_err(|e| format!("Read body: {e}"))?;
    let data: DefiLlamaYieldResponse =
        serde_json::from_str(&body).map_err(|e| format!("Parse DefiLlama response: {e}"))?;

    let mut yields_by_token: HashMap<String, Vec<YieldInfo>> = HashMap::new();

    for pool in data.data {
        if pool.chain.as_deref() != Some("Solana") {
            continue;
        }

        let project = match &pool.project {
            Some(p) => p.clone(),
            None => continue,
        };

        let symbol = pool.symbol.clone().unwrap_or_else(|| "Unknown".to_string());
        let dex_name = DexName::from_project(&project);

        let yield_info = YieldInfo {
            symbol: symbol.clone(),
            project: project.clone(),
            dex_name,
            apy: pool.apy,
            apy_base: pool.apy_base,
            apy_reward: pool.apy_reward,
            tvl_usd: pool.tvl_usd.unwrap_or(0.0),
            il_risk: pool.il_risk.as_deref() == Some("yes"),
            volume_24h: pool.volume_usd_1d,
            apy_mean_30d: pool.apy_mean_30d,
            underlying_tokens: pool.underlying_tokens.unwrap_or_default(),
        };

        for token in &yield_info.underlying_tokens {
            yields_by_token
                .entry(token.clone())
                .or_default()
                .push(yield_info.clone());
        }
    }

    for (_token, yields) in yields_by_token.iter_mut() {
        yields.sort_by(|a, b| {
            b.apy
                .unwrap_or(0.0)
                .partial_cmp(&a.apy.unwrap_or(0.0))
                .unwrap()
        });
    }

    debug!("Loaded {} tokens with yield data", yields_by_token.len());
    Ok(yields_by_token)
}

pub fn match_yield_to_pair(
    base_token: &Option<String>,
    quote_token: &Option<String>,
    yields: &HashMap<String, Vec<YieldInfo>>,
) -> Vec<YieldInfo> {
    let mut matches = Vec::new();
    let mut seen_keys: HashSet<String> = HashSet::new();

    if let (Some(base), Some(quote)) = (base_token, quote_token) {
        if let Some(base_yields) = yields.get(base) {
            for y in base_yields {
                if y.underlying_tokens.contains(quote) {
                    let key = format!("{}-{}", y.symbol, y.project);
                    if seen_keys.insert(key) {
                        matches.push(y.clone());
                    }
                }
            }
        }

        if let Some(quote_yields) = yields.get(quote) {
            for y in quote_yields {
                if y.underlying_tokens.contains(base) {
                    let key = format!("{}-{}", y.symbol, y.project);
                    if seen_keys.insert(key) {
                        matches.push(y.clone());
                    }
                }
            }
        }
    }

    matches.sort_by(|a, b| {
        b.apy
            .unwrap_or(0.0)
            .partial_cmp(&a.apy.unwrap_or(0.0))
            .unwrap()
    });

    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dex_name_from_project() {
        assert_eq!(DexName::from_project("meteora-dlmm"), DexName::Meteora);
        assert_eq!(DexName::from_project("raydium-amm"), DexName::Raydium);
        assert_eq!(DexName::from_project("orca-dex"), DexName::Orca);
        assert_eq!(DexName::from_project("kamino-liquidity"), DexName::Kamino);
        assert_eq!(DexName::from_project("unknown-protocol"), DexName::Other);
    }

    #[test]
    fn test_yield_info_display_apy() {
        let y = YieldInfo {
            symbol: "SOL-USDC".to_string(),
            project: "raydium".to_string(),
            dex_name: DexName::Raydium,
            apy: Some(12.5),
            apy_base: Some(10.0),
            apy_reward: Some(2.5),
            tvl_usd: 1_000_000.0,
            il_risk: true,
            volume_24h: Some(500_000.0),
            apy_mean_30d: Some(11.0),
            underlying_tokens: vec!["SOL".to_string(), "USDC".to_string()],
        };

        assert_eq!(y.display_apy(), "12.50%");
    }

    #[test]
    fn test_match_yield_to_pair() {
        let mut yields: HashMap<String, Vec<YieldInfo>> = HashMap::new();

        let y1 = YieldInfo {
            symbol: "SOL-USDC".to_string(),
            project: "raydium".to_string(),
            dex_name: DexName::Raydium,
            apy: Some(15.0),
            apy_base: Some(12.0),
            apy_reward: Some(3.0),
            tvl_usd: 2_000_000.0,
            il_risk: true,
            volume_24h: Some(800_000.0),
            apy_mean_30d: Some(14.0),
            underlying_tokens: vec!["SOL_ADDR".to_string(), "USDC_ADDR".to_string()],
        };

        yields
            .entry("SOL_ADDR".to_string())
            .or_default()
            .push(y1.clone());
        yields.entry("USDC_ADDR".to_string()).or_default().push(y1);

        let matches = match_yield_to_pair(
            &Some("SOL_ADDR".to_string()),
            &Some("USDC_ADDR".to_string()),
            &yields,
        );

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].apy, Some(15.0));
    }
}
