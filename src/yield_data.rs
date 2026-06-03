/// Yield data integration via DefiLlama Yields API.
///
/// Fetch APY/fee data for Solana LP pools and match to DexScreener pairs
/// by underlying token addresses. Also provides DEX profile context
/// (pool type, fee tier, IL behavior, strengths) for Meteora, Raydium, Orca, etc.
use crate::api::PairData;
use serde::Deserialize;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct DefiLlamaPool {
    #[serde(default)]
    pub pool: String,
    #[serde(default)]
    pub chain: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub symbol: String,
    #[serde(rename = "tvlUsd", default)]
    pub tvl_usd: f64,
    #[serde(rename = "apyBase")]
    pub apy_base: Option<f64>,
    #[serde(rename = "apyReward")]
    pub apy_reward: Option<f64>,
    pub apy: Option<f64>,
    #[serde(rename = "ilRisk", default)]
    pub il_risk: String,
    #[serde(rename = "volumeUsd1d")]
    pub volume_usd_1d: Option<f64>,
    #[serde(rename = "apyMean30d")]
    pub apy_mean_30d: Option<f64>,
    #[serde(rename = "underlyingTokens", default)]
    pub underlying_tokens: Vec<String>,
    #[serde(rename = "poolMeta")]
    pub pool_meta: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DefiLlamaResponse {
    pub data: Vec<DefiLlamaPool>,
}

/// Matched yield info for a specific pair.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct YieldInfo {
    /// Total APY (fee + reward)
    pub apy: Option<f64>,
    /// Organic fee APY
    pub apy_base: Option<f64>,
    /// Incentive/reward APY
    pub apy_reward: Option<f64>,
    /// 30-day mean APY for stability context
    pub apy_mean_30d: Option<f64>,
    /// Pool TVL
    pub tvl_usd: f64,
    /// Whether IL risk is flagged
    pub il_risk: bool,
    /// DefiLlama project identifier ("raydium-amm", "orca-dex", etc.)
    pub project: String,
    /// Pool meta (fee tier like "0.05%", "Stable", etc.)
    pub pool_meta: Option<String>,
}

impl YieldInfo {
    /// One-line summary for display: "APY 12.5% (fee 10.2% + reward 2.3%)"
    pub fn display(&self) -> String {
        let total = self.apy.map(|v| format!("{v:.1}%")).unwrap_or_else(|| "--".to_string());
        let base = self.apy_base.map(|v| format!("{v:.1}%")).unwrap_or_else(|| "--".to_string());
        let reward = self.apy_reward.unwrap_or(0.0);
        if reward > 0.1 {
            format!("APY {total} (fee {base} + reward {reward:.1}%)")
        } else {
            format!("APY {total} (fee {base})")
        }
    }

    /// 30d mean note for context.
    pub fn stability_note(&self) -> Option<String> {
        let mean = self.apy_mean_30d?;
        let current = self.apy?;
        let diff_pct = ((current - mean) / mean.abs().max(0.001)) * 100.0;
        if diff_pct > 50.0 {
            Some(format!("APY 30d mean {mean:.1}% — spike saat ini ({diff_pct:+.0}% di atas rerata)"))
        } else if diff_pct < -50.0 {
            Some(format!("APY 30d mean {mean:.1}% — saat ini jauh di bawah rerata ({diff_pct:+.0}%)"))
        } else {
            Some(format!("APY 30d mean {mean:.1}%"))
        }
    }
}

// ---------------------------------------------------------------------------
// DEX profile differentiation
// ---------------------------------------------------------------------------

/// Characteristic profile for each major Solana DEX.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DexProfile {
    /// Human-readable DEX name
    pub name: &'static str,
    /// Pool/AMM model description
    pub pool_type: &'static str,
    /// Fee model summary
    pub fee_model: &'static str,
    /// IL risk level: Low/Moderate/High
    pub il_risk_level: &'static str,
    /// Key strength for LP
    pub strength: &'static str,
    /// Key caution for LP
    pub caution: &'static str,
}

pub fn dex_profile(dex_id: &str) -> DexProfile {
    let normalized = dex_id.to_ascii_lowercase();

    if normalized.contains("meteora") {
        DexProfile {
            name: "Meteora",
            pool_type: "DLMM (Dynamic Liquidity Market Maker) / Dynamic pools",
            fee_model: "Fee dinamis — naik saat volatilitas tinggi, turun saat stabil",
            il_risk_level: "Moderate–High (DLMM: fee tinggi saat vol tinggi, tapi IL tetap ada)",
            strength: "Fee tinggi saat pasar volatile; cocok untuk pair aktif",
            caution: "Fee bisa turun drastis saat pasar tenang; posisi perlu monitoring",
        }
    } else if normalized.contains("raydium") {
        DexProfile {
            name: "Raydium",
            pool_type: "CLMM (Concentrated Liquidity) + CPMM (Standard AMM)",
            fee_model: "Fixed fee tier: 0.01% / 0.05% / 0.25% / 1% tergantung pool",
            il_risk_level: "Moderate (CLMM: IL tinggi jika harga keluar range; CPMM: IL standar)",
            strength: "Volume tinggi, pair utama (SOL/USDC) yield organik konsisten",
            caution: "CLMM memerlukan manajemen range aktif; idle jika harga di luar range",
        }
    } else if normalized.contains("orca") {
        DexProfile {
            name: "Orca",
            pool_type: "Whirlpools (CLMM) — Concentrated Liquidity",
            fee_model: "Fixed fee tier: 0.01% / 0.05% / 0.3% / 1%",
            il_risk_level: "Moderate (CLMM: IL tergantung width range yang dipilih)",
            strength: "UX ramah, tick spacing fleksibel, cocok untuk range lebar",
            caution: "Likuiditas lebih tipis dari Raydium untuk pair kecil; waspada slippage",
        }
    } else if normalized.contains("pumpswap") || normalized.contains("pump") {
        DexProfile {
            name: "PumpSwap",
            pool_type: "AMM standar (bonding curve pada fase early, AMM setelah graduation)",
            fee_model: "Fee 0.25% — sebagian ke LP, sebagian ke protokol",
            il_risk_level: "High (token baru, volatilitas ekstrem, rug risk)",
            strength: "Early entry ke token baru yang bisa 100x",
            caution: "Risiko rug pull sangat tinggi; majority token gagal mencapai graduation",
        }
    } else if normalized.contains("lifinity") {
        DexProfile {
            name: "Lifinity",
            pool_type: "Proactive Market Maker (PMM) dengan oracle pricing",
            fee_model: "Fee variabel, tergantung imbalance pool",
            il_risk_level: "Low–Moderate (oracle-based pricing mengurangi IL dari arbitrage)",
            strength: "IL lebih rendah dari AMM standar; cocok untuk stablecoin dan pair besar",
            caution: "Ketergantungan oracle; perlu cek oracle health; likuiditas lebih terbatas",
        }
    } else if normalized.contains("phoenix") {
        DexProfile {
            name: "Phoenix",
            pool_type: "CLOB (Central Limit Order Book) on-chain",
            fee_model: "Maker/taker fee, biasanya 0–0.1% maker, 0.1% taker",
            il_risk_level: "Low (CLOB tidak expose LP ke IL seperti AMM)",
            strength: "Spread ketat untuk pair utama; cocok untuk market maker serius",
            caution: "Kompleks, butuh strategi aktif; tidak pasif seperti AMM LP",
        }
    } else {
        DexProfile {
            name: "Unknown DEX",
            pool_type: "AMM / DEX belum terklasifikasi",
            fee_model: "Fee tidak diketahui — cek dokumentasi DEX",
            il_risk_level: "Tidak diketahui",
            strength: "Data DEX terbatas",
            caution: "Verifikasi manual sebelum entry",
        }
    }
}

/// Format DEX profile for display in the analysis panel.
pub fn dex_profile_summary(dex_id: &str) -> String {
    let p = dex_profile(dex_id);
    format!(
        "{} | {} | Fee: {} | IL: {}",
        p.name, p.pool_type, p.fee_model, p.il_risk_level
    )
}

// ---------------------------------------------------------------------------
// Yield fetch
// ---------------------------------------------------------------------------

/// Fetch all Solana pools from DefiLlama Yields API.
/// Returns a vector of pools (filtered to chain=Solana).
pub async fn fetch_solana_yields(client: &reqwest::Client) -> Result<Vec<DefiLlamaPool>, String> {
    let resp = client
        .get("https://yields.llama.fi/pools")
        .header("User-Agent", "volatile-scanner/0.1")
        .send()
        .await
        .map_err(|e| format!("DefiLlama fetch error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("DefiLlama API status: {}", resp.status()));
    }

    let body = resp.text().await.map_err(|e| format!("DefiLlama read body: {e}"))?;
    let data: DefiLlamaResponse =
        serde_json::from_str(&body).map_err(|e| format!("DefiLlama parse: {e}"))?;

    // Filter Solana only
    let solana_pools: Vec<DefiLlamaPool> = data
        .data
        .into_iter()
        .filter(|p| p.chain.eq_ignore_ascii_case("solana"))
        .collect();

    Ok(solana_pools)
}

// ---------------------------------------------------------------------------
// Matching logic
// ---------------------------------------------------------------------------

/// Build a lookup map: (base_addr_lower, quote_addr_lower) -> best YieldInfo.
/// "Best" = highest TVL among all matching pools.
pub fn build_yield_index(pools: Vec<DefiLlamaPool>) -> HashMap<(String, String), YieldInfo> {
    let mut map: HashMap<(String, String), YieldInfo> = HashMap::new();

    for pool in pools {
        if pool.underlying_tokens.len() < 2 {
            continue;
        }

        let tokens: Vec<String> = pool
            .underlying_tokens
            .iter()
            .map(|t| t.to_ascii_lowercase())
            .collect();

        let info = YieldInfo {
            apy: pool.apy,
            apy_base: pool.apy_base,
            apy_reward: pool.apy_reward,
            apy_mean_30d: pool.apy_mean_30d,
            tvl_usd: pool.tvl_usd,
            il_risk: pool.il_risk.eq_ignore_ascii_case("yes"),
            project: pool.project.clone(),
            pool_meta: pool.pool_meta.clone(),
        };

        // Store both orderings (A,B) and (B,A) for symmetric lookup
        for i in 0..tokens.len() {
            for j in 0..tokens.len() {
                if i == j {
                    continue;
                }
                let key = (tokens[i].clone(), tokens[j].clone());
                let entry = map.entry(key).or_insert_with(|| info.clone());
                // Prefer higher TVL pool when multiple matches
                if info.tvl_usd > entry.tvl_usd {
                    *entry = info.clone();
                }
            }
        }
    }

    map
}

/// Match a DexScreener pair to yield info using token address matching.
pub fn match_yield(
    pair: &PairData,
    index: &HashMap<(String, String), YieldInfo>,
) -> Option<YieldInfo> {
    let base = pair.base_token.address.as_deref()?.to_ascii_lowercase();
    let quote = pair.quote_token.address.as_deref()?.to_ascii_lowercase();
    index.get(&(base.clone(), quote.clone()))
        .or_else(|| index.get(&(quote, base)))
        .cloned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{PairData, PriceChange, TokenInfo};

    fn make_pair(base_addr: &str, quote_addr: &str) -> PairData {
        PairData {
            chain_id: "solana".to_string(),
            dex_id: "raydium".to_string(),
            base_token: TokenInfo {
                name: Some("BASE".to_string()),
                symbol: Some("BASE".to_string()),
                address: Some(base_addr.to_string()),
            },
            quote_token: TokenInfo {
                name: Some("USDC".to_string()),
                symbol: Some("USDC".to_string()),
                address: Some(quote_addr.to_string()),
            },
            price_usd: Some("1.0".to_string()),
            price_change: Some(PriceChange { m5: None, h1: None, h6: None, h24: None }),
            txns: None,
            volume: None,
            liquidity: None,
            market_cap: None,
            fdv: None,
            pair_created_at: None,
            pair_address: None,
            url: None,
        }
    }

    fn make_pool(base: &str, quote: &str, tvl: f64, apy: f64) -> DefiLlamaPool {
        DefiLlamaPool {
            pool: "test-pool".to_string(),
            chain: "Solana".to_string(),
            project: "raydium-amm".to_string(),
            symbol: "BASE-USDC".to_string(),
            tvl_usd: tvl,
            apy_base: Some(apy),
            apy_reward: Some(0.0),
            apy: Some(apy),
            il_risk: "yes".to_string(),
            volume_usd_1d: None,
            apy_mean_30d: Some(apy * 0.9),
            underlying_tokens: vec![base.to_string(), quote.to_string()],
            pool_meta: None,
        }
    }

    #[test]
    fn test_build_yield_index_matches_forward() {
        let pool = make_pool("AAAA", "BBBB", 100_000.0, 12.5);
        let index = build_yield_index(vec![pool]);
        let pair = make_pair("AAAA", "BBBB");
        let result = match_yield(&pair, &index);
        assert!(result.is_some());
        assert!((result.unwrap().apy.unwrap() - 12.5).abs() < 1e-6);
    }

    #[test]
    fn test_build_yield_index_matches_reverse() {
        // Match should work even when addresses are in reverse order
        let pool = make_pool("AAAA", "BBBB", 100_000.0, 10.0);
        let index = build_yield_index(vec![pool]);
        let pair = make_pair("BBBB", "AAAA"); // reversed
        let result = match_yield(&pair, &index);
        assert!(result.is_some());
    }

    #[test]
    fn test_build_yield_index_prefers_higher_tvl() {
        let pool_low = make_pool("AAAA", "BBBB", 50_000.0, 5.0);
        let pool_high = make_pool("AAAA", "BBBB", 200_000.0, 15.0);
        let index = build_yield_index(vec![pool_low, pool_high]);
        let pair = make_pair("AAAA", "BBBB");
        let result = match_yield(&pair, &index).unwrap();
        assert!((result.tvl_usd - 200_000.0).abs() < 1.0);
    }

    #[test]
    fn test_match_yield_no_match_returns_none() {
        let pool = make_pool("AAAA", "BBBB", 100_000.0, 10.0);
        let index = build_yield_index(vec![pool]);
        let pair = make_pair("CCCC", "DDDD");
        assert!(match_yield(&pair, &index).is_none());
    }

    #[test]
    fn test_match_yield_no_address_returns_none() {
        let index = build_yield_index(vec![]);
        let mut pair = make_pair("", "");
        pair.base_token.address = None;
        assert!(match_yield(&pair, &index).is_none());
    }

    #[test]
    fn test_yield_info_display_with_reward() {
        let info = YieldInfo {
            apy: Some(15.0),
            apy_base: Some(12.0),
            apy_reward: Some(3.0),
            apy_mean_30d: Some(14.0),
            tvl_usd: 100_000.0,
            il_risk: true,
            project: "raydium-amm".to_string(),
            pool_meta: None,
        };
        let d = info.display();
        assert!(d.contains("15.0%"));
        assert!(d.contains("12.0%"));
        assert!(d.contains("reward"));
    }

    #[test]
    fn test_yield_info_display_fee_only() {
        let info = YieldInfo {
            apy: Some(8.5),
            apy_base: Some(8.5),
            apy_reward: Some(0.0),
            apy_mean_30d: None,
            tvl_usd: 50_000.0,
            il_risk: false,
            project: "orca-dex".to_string(),
            pool_meta: None,
        };
        let d = info.display();
        assert!(d.contains("8.5%"));
        assert!(!d.contains("reward"));
    }

    #[test]
    fn test_yield_info_stability_note_spike() {
        let info = YieldInfo {
            apy: Some(150.0),
            apy_base: Some(150.0),
            apy_reward: None,
            apy_mean_30d: Some(50.0),
            tvl_usd: 10_000.0,
            il_risk: true,
            project: "meteora".to_string(),
            pool_meta: None,
        };
        let note = info.stability_note().unwrap();
        assert!(note.contains("spike"));
    }

    #[test]
    fn test_yield_info_stability_note_normal() {
        let info = YieldInfo {
            apy: Some(10.0),
            apy_base: Some(10.0),
            apy_reward: None,
            apy_mean_30d: Some(9.5),
            tvl_usd: 100_000.0,
            il_risk: false,
            project: "orca-dex".to_string(),
            pool_meta: None,
        };
        let note = info.stability_note().unwrap();
        // Small diff, just shows mean
        assert!(note.contains("9.5%"));
        assert!(!note.contains("spike"));
    }

    // DEX profile tests

    #[test]
    fn test_dex_profile_meteora() {
        let p = dex_profile("meteora");
        assert_eq!(p.name, "Meteora");
        assert!(p.pool_type.contains("DLMM"));
    }

    #[test]
    fn test_dex_profile_raydium() {
        let p = dex_profile("raydium");
        assert!(p.name.contains("Raydium"));
        assert!(p.pool_type.contains("CLMM"));
    }

    #[test]
    fn test_dex_profile_orca() {
        let p = dex_profile("orca");
        assert_eq!(p.name, "Orca");
        assert!(p.pool_type.contains("Whirlpool"));
    }

    #[test]
    fn test_dex_profile_pumpswap() {
        let p = dex_profile("pumpswap");
        assert!(p.il_risk_level.contains("High"));
    }

    #[test]
    fn test_dex_profile_unknown() {
        let p = dex_profile("unknowndex");
        assert!(p.pool_type.contains("belum terklasifikasi"));
        assert_eq!(p.name, "Unknown DEX");
    }

    #[test]
    fn test_dex_profile_summary_contains_dex_name() {
        let s = dex_profile_summary("raydium");
        assert!(s.to_ascii_lowercase().contains("raydium"));
    }

    #[test]
    fn test_il_risk_parsed_correctly() {
        let pool = make_pool("A", "B", 1000.0, 10.0);
        let index = build_yield_index(vec![pool]);
        let pair = make_pair("A", "B");
        let result = match_yield(&pair, &index).unwrap();
        assert!(result.il_risk); // "yes" → true
    }

    #[test]
    fn test_case_insensitive_address_matching() {
        let pool = make_pool("AaAaBbBb", "CcCcDdDd", 100_000.0, 8.0);
        let index = build_yield_index(vec![pool]);
        let pair = make_pair("aaaabbbb", "ccccdddd");
        assert!(match_yield(&pair, &index).is_some());
    }
}
