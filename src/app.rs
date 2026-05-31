use crate::api::PairData;
use chrono::Local;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq)]
pub enum SortColumn {
    Symbol,
    Price,
    Change24h,
    Change1h,
    Volume24h,
    Liquidity,
    Fdv,
    Signal,
    Age,
}

impl SortColumn {
    pub fn next(&self) -> Self {
        match self {
            SortColumn::Symbol => SortColumn::Price,
            SortColumn::Price => SortColumn::Change24h,
            SortColumn::Change24h => SortColumn::Change1h,
            SortColumn::Change1h => SortColumn::Volume24h,
            SortColumn::Volume24h => SortColumn::Liquidity,
            SortColumn::Liquidity => SortColumn::Fdv,
            SortColumn::Fdv => SortColumn::Signal,
            SortColumn::Signal => SortColumn::Age,
            SortColumn::Age => SortColumn::Symbol,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            SortColumn::Symbol => SortColumn::Age,
            SortColumn::Price => SortColumn::Symbol,
            SortColumn::Change24h => SortColumn::Price,
            SortColumn::Change1h => SortColumn::Change24h,
            SortColumn::Volume24h => SortColumn::Change1h,
            SortColumn::Liquidity => SortColumn::Volume24h,
            SortColumn::Fdv => SortColumn::Liquidity,
            SortColumn::Signal => SortColumn::Fdv,
            SortColumn::Age => SortColumn::Signal,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    pub fn toggle(&self) -> Self {
        match self {
            SortDirection::Ascending => SortDirection::Descending,
            SortDirection::Descending => SortDirection::Ascending,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeSignal {
    GoodEntry,
    Watchlist,
    Avoid,
}

impl TradeSignal {
    pub fn label(&self) -> &'static str {
        match self {
            TradeSignal::GoodEntry => "LAYAK MASUK",
            TradeSignal::Watchlist => "PANTAU",
            TradeSignal::Avoid => "HINDARI",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PairAssessment {
    pub signal: TradeSignal,
    pub score: i32,
    pub bias_summary: String,
    pub notes: Vec<String>,
}

pub struct App {
    pub pairs: Vec<PairData>,
    pub assessments: Vec<PairAssessment>,
    pub filtered_pairs: Vec<usize>,
    pub sort_column: SortColumn,
    pub sort_direction: SortDirection,
    pub selected_index: usize,
    pub status_message: String,
    pub last_update: String,
    pub min_volume_filter: f64,
    pub min_liquidity_filter: f64,
    pub search_query: String,
    pub search_mode: bool,
    pub open_link_mode: bool,
    pub is_loading: bool,
    pub show_help: bool,
    pub error_message: Option<String>,
}

impl App {
    pub fn new() -> Self {
        App {
            pairs: Vec::new(),
            assessments: Vec::new(),
            filtered_pairs: Vec::new(),
            sort_column: SortColumn::Change24h,
            sort_direction: SortDirection::Descending,
            selected_index: 0,
            status_message: String::from("Initializing..."),
            last_update: String::from("--"),
            min_volume_filter: 1000.0,
            min_liquidity_filter: 500.0,
            search_query: String::new(),
            search_mode: false,
            open_link_mode: false,
            is_loading: true,
            show_help: false,
            error_message: None,
        }
    }

    pub fn update_pairs(&mut self, pairs: Vec<PairData>) {
        self.pairs = pairs;
        self.assessments = self.pairs.iter().map(assess_pair).collect();
        self.apply_filters();
        self.last_update = Local::now().format("%H:%M:%S").to_string();
        let count = self.pairs.len();
        self.status_message = format!("Loaded {count} pairs");
        self.is_loading = false;
        self.error_message = None;

        if self.selected_index >= self.filtered_pairs.len() {
            self.selected_index = self.filtered_pairs.len().saturating_sub(1);
        }
    }

    pub fn set_warning(&mut self, msg: String) {
        self.status_message = msg;
    }

    pub fn set_error(&mut self, msg: String) {
        self.error_message = Some(msg.clone());
        self.status_message = msg;
        self.is_loading = false;
    }

    pub fn apply_filters(&mut self) {
        let query = self.search_query.to_lowercase();
        let mut indices: Vec<usize> = self
            .pairs
            .iter()
            .enumerate()
            .filter(|(_, pair)| {
                let vol = pair.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
                let liq = pair.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0);

                if vol < self.min_volume_filter || liq < self.min_liquidity_filter {
                    return false;
                }

                if !query.is_empty() {
                    let symbol = pair
                        .base_token
                        .symbol
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase();
                    let name = pair.base_token.name.as_deref().unwrap_or("").to_lowercase();
                    if !symbol.contains(&query) && !name.contains(&query) {
                        return false;
                    }
                }

                true
            })
            .map(|(i, _)| i)
            .collect();

        self.sort_indices(&mut indices);
        self.filtered_pairs = indices;
        let filtered = self.filtered_pairs.len();
        let total = self.pairs.len();
        let vol = self.min_volume_filter;
        let liq = self.min_liquidity_filter;
        self.status_message =
            format!("Showing {filtered}/{total} pairs | Vol >= ${vol:.0} | Liq >= ${liq:.0}");
    }

    fn sort_indices(&self, indices: &mut [usize]) {
        let pairs = &self.pairs;
        let desc = self.sort_direction == SortDirection::Descending;

        indices.sort_by(|&a, &b| {
            let pa = &pairs[a];
            let pb = &pairs[b];
            let cmp = match self.sort_column {
                SortColumn::Symbol => {
                    let sym_a = pa.base_token.symbol.as_deref().unwrap_or("");
                    let sym_b = pb.base_token.symbol.as_deref().unwrap_or("");
                    sym_a.cmp(sym_b)
                }
                SortColumn::Price => {
                    let price_a = parse_usd(&pa.price_usd);
                    let price_b = parse_usd(&pb.price_usd);
                    price_a
                        .partial_cmp(&price_b)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::Change24h => {
                    let ch_a = pa
                        .price_change
                        .as_ref()
                        .and_then(|pc| pc.h24)
                        .unwrap_or(0.0);
                    let ch_b = pb
                        .price_change
                        .as_ref()
                        .and_then(|pc| pc.h24)
                        .unwrap_or(0.0);
                    ch_a.partial_cmp(&ch_b).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::Change1h => {
                    let ch_a = pa.price_change.as_ref().and_then(|pc| pc.h1).unwrap_or(0.0);
                    let ch_b = pb.price_change.as_ref().and_then(|pc| pc.h1).unwrap_or(0.0);
                    ch_a.partial_cmp(&ch_b).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::Volume24h => {
                    let vol_a = pa.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
                    let vol_b = pb.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
                    vol_a
                        .partial_cmp(&vol_b)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::Liquidity => {
                    let liq_a = pa.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0);
                    let liq_b = pb.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0);
                    liq_a
                        .partial_cmp(&liq_b)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::Fdv => {
                    let fdv_a = pa.fdv.unwrap_or(0.0);
                    let fdv_b = pb.fdv.unwrap_or(0.0);
                    fdv_a
                        .partial_cmp(&fdv_b)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::Signal => {
                    let signal_a = self.assessments.get(a).map(|x| x.score).unwrap_or_default();
                    let signal_b = self.assessments.get(b).map(|x| x.score).unwrap_or_default();
                    signal_a.cmp(&signal_b)
                }
                SortColumn::Age => {
                    let age_a = pa.pair_created_at.unwrap_or(0);
                    let age_b = pb.pair_created_at.unwrap_or(0);
                    age_a.cmp(&age_b)
                }
            };
            if desc {
                cmp.reverse()
            } else {
                cmp
            }
        });
    }

    pub fn selected_pair(&self) -> Option<&PairData> {
        self.filtered_pairs
            .get(self.selected_index)
            .map(|&i| &self.pairs[i])
    }

    pub fn selected_assessment(&self) -> Option<PairAssessment> {
        self.filtered_pairs
            .get(self.selected_index)
            .and_then(|&idx| self.assessments.get(idx))
            .cloned()
    }

    pub fn assessment_for_pair_index(&self, idx: usize) -> Option<&PairAssessment> {
        self.assessments.get(idx)
    }

    pub fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_selection_down(&mut self) {
        if self.selected_index + 1 < self.filtered_pairs.len() {
            self.selected_index += 1;
        }
    }

    pub fn toggle_sort_direction(&mut self) {
        self.sort_direction = self.sort_direction.toggle();
        self.apply_filters();
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn start_search(&mut self) {
        self.search_mode = true;
        self.status_message = String::from("Search mode: type query, Enter to keep, Esc to clear");
    }

    pub fn stop_search_keep_query(&mut self) {
        self.search_mode = false;
        self.apply_filters();
    }

    pub fn stop_search_clear_query(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.apply_filters();
    }

    pub fn start_open_link_mode(&mut self) {
        self.open_link_mode = true;
        self.status_message =
            String::from("Open mode: press [m] Meteora, [d] DexScreener, [Esc] cancel");
    }

    pub fn stop_open_link_mode(&mut self) {
        self.open_link_mode = false;
    }
}

fn parse_usd(s: &Option<String>) -> f64 {
    s.as_ref()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0)
}

const SCORE_BASELINE: i32 = 50;
const SCORE_GOOD_ENTRY_MIN: i32 = 67;
const SCORE_WATCHLIST_MIN: i32 = 45;

const LIQUIDITY_STRONG_USD: f64 = 150_000.0;
const LIQUIDITY_THIN_USD: f64 = 20_000.0;
const VOLUME_ACTIVE_USD: f64 = 500_000.0;
const VOLUME_WEAK_USD: f64 = 50_000.0;

const MOMENTUM_HEALTHY_24H_MIN: f64 = 5.0;
const MOMENTUM_HEALTHY_24H_MAX: f64 = 35.0;
const MOMENTUM_HEALTHY_1H_MIN: f64 = 0.5;
const MOMENTUM_HEALTHY_1H_MAX: f64 = 8.0;

const MOMENTUM_EXTENSION_24H_MIN: f64 = 35.0;
const MOMENTUM_EXTENSION_24H_MAX: f64 = 60.0;
const MOMENTUM_BLOWOFF_24H_MIN: f64 = 60.0;
const MOMENTUM_BEARISH_24H_MIN: f64 = -12.0;
const MOMENTUM_FLAT_24H_MAX_ABS: f64 = 3.0;
const MOMENTUM_FLAT_1H_MAX_ABS: f64 = 1.0;
const MOMENTUM_5M_REBOUND_MIN: f64 = 2.0;
const MOMENTUM_5M_DUMP_MAX: f64 = -4.0;

const VOL_LIQ_RATIO_HEALTHY_MIN: f64 = 0.7;
const VOL_LIQ_RATIO_HEALTHY_MAX: f64 = 8.0;
const VOL_LIQ_RATIO_OVERHEATED_MIN: f64 = 15.0;
const PRESSURE_DOMINANCE_RATIO: f64 = 1.2;
const PRESSURE_MIN_CONFIDENCE_TXNS: f64 = 15.0;
const AGE_HOURS_VERY_NEW: f64 = 1.0;
const AGE_HOURS_NEW: f64 = 6.0;
const AGE_HOURS_YOUNG: f64 = 24.0;
const AGE_HOURS_EARLY: f64 = 7.0 * 24.0;
const AGE_PENALTY_VERY_NEW: i32 = 25;
const AGE_PENALTY_NEW: i32 = 15;
const AGE_PENALTY_YOUNG: i32 = 8;
const AGE_PENALTY_EARLY: i32 = 3;
const QUOTE_RISK_PENALTY: i32 = 4;
const KNOWN_QUOTES: [&str; 3] = ["USDC", "USDT", "SOL"];

fn symbol_or_unknown(symbol: Option<&str>) -> &str {
    symbol.unwrap_or("UNKNOWN")
}

fn weighted_pressure(pair: &PairData) -> Option<(f64, f64)> {
    let txns = pair.txns.as_ref()?;
    let m5 = txns.m5.as_ref();
    let h1 = txns.h1.as_ref();
    let h24 = txns.h24.as_ref();

    let mut buys = 0.0;
    let mut sells = 0.0;
    let mut weight_sum = 0.0;

    if let Some(window) = h1 {
        buys += window.buys.unwrap_or(0.0) * 0.7;
        sells += window.sells.unwrap_or(0.0) * 0.7;
        weight_sum += 0.7;
    }
    if let Some(window) = m5 {
        buys += window.buys.unwrap_or(0.0) * 0.3;
        sells += window.sells.unwrap_or(0.0) * 0.3;
        weight_sum += 0.3;
    }

    if weight_sum > 0.0 {
        return Some((buys, sells));
    }

    h24.map(|window| (window.buys.unwrap_or(0.0), window.sells.unwrap_or(0.0)))
}

fn pressure_bias_summary(pair: &PairData) -> String {
    let base_symbol = symbol_or_unknown(pair.base_token.symbol.as_deref());
    let quote_symbol = symbol_or_unknown(pair.quote_token.symbol.as_deref());

    let Some((buys, sells)) = weighted_pressure(pair) else {
        return format!(
            "Data tekanan beli/jual belum cukup untuk pasangan {base_symbol}/{quote_symbol}."
        );
    };

    let total = buys + sells;
    if total < PRESSURE_MIN_CONFIDENCE_TXNS {
        return format!(
            "Aktivitas tx rendah pada {base_symbol}/{quote_symbol} (buy {buys:.1} vs sell {sells:.1}); tunggu konfirmasi."
        );
    }

    if sells > buys * PRESSURE_DOMINANCE_RATIO {
        return format!(
            "Tekanan jual dominan (buy {buys:.1} vs sell {sells:.1}) pada {base_symbol}/{quote_symbol} -> bias pilih {base_symbol} (strategi entry saat diskon)."
        );
    }

    if buys > sells * PRESSURE_DOMINANCE_RATIO {
        return format!(
            "Tekanan beli dominan (buy {buys:.1} vs sell {sells:.1}) pada {base_symbol}/{quote_symbol} -> bias pilih {quote_symbol} (strategi take profit ke aset quote)."
        );
    }

    format!(
        "Tekanan beli/jual relatif seimbang (buy {buys:.1} vs sell {sells:.1}) pada {base_symbol}/{quote_symbol}."
    )
}

fn pair_age_hours(pair: &PairData) -> Option<f64> {
    let created_at_ms = pair.pair_created_at?;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    if created_at_ms >= now_ms {
        return Some(0.0);
    }
    let age_ms = now_ms.saturating_sub(created_at_ms);
    Some(age_ms as f64 / 3_600_000.0)
}

fn is_known_quote_symbol(pair: &PairData) -> bool {
    let quote = pair
        .quote_token
        .symbol
        .as_deref()
        .unwrap_or("")
        .to_ascii_uppercase();
    KNOWN_QUOTES.iter().any(|known| known == &quote)
}

pub fn assess_pair(pair: &PairData) -> PairAssessment {
    let change_24h = pair
        .price_change
        .as_ref()
        .and_then(|pc| pc.h24)
        .unwrap_or(0.0);
    let change_1h = pair
        .price_change
        .as_ref()
        .and_then(|pc| pc.h1)
        .unwrap_or(0.0);
    let change_5m = pair
        .price_change
        .as_ref()
        .and_then(|pc| pc.m5)
        .unwrap_or(0.0);
    let vol_24h = pair.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
    let liquidity = pair.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0);
    let vol_liq_ratio = if liquidity > 0.0 {
        vol_24h / liquidity
    } else {
        0.0
    };

    let mut score = SCORE_BASELINE;
    let mut notes = Vec::new();
    let bias_summary = pressure_bias_summary(pair);

    if liquidity >= LIQUIDITY_STRONG_USD {
        score += 12;
        notes.push("Likuiditas kuat, risiko slippage lebih rendah.".to_string());
    } else if liquidity < LIQUIDITY_THIN_USD {
        score -= 20;
        notes.push("Likuiditas tipis, slippage dan wick bisa tinggi.".to_string());
    }

    if vol_24h >= VOLUME_ACTIVE_USD {
        score += 10;
        notes.push("Volume 24h tinggi, market aktif.".to_string());
    } else if vol_24h < VOLUME_WEAK_USD {
        score -= 12;
        notes.push("Volume rendah, setup kurang valid.".to_string());
    }

    if (MOMENTUM_HEALTHY_24H_MIN..=MOMENTUM_HEALTHY_24H_MAX).contains(&change_24h)
        && (MOMENTUM_HEALTHY_1H_MIN..=MOMENTUM_HEALTHY_1H_MAX).contains(&change_1h)
        && change_5m > -2.0
    {
        score += 18;
        notes.push("Momentum naik sehat (24h + 1h selaras).".to_string());
    } else if (MOMENTUM_EXTENSION_24H_MIN..=MOMENTUM_EXTENSION_24H_MAX).contains(&change_24h)
        && change_1h > 0.0
    {
        score += 10;
        notes.push("Momentum naik lanjutan, tapi mulai extended.".to_string());
    } else if change_24h > MOMENTUM_BLOWOFF_24H_MIN && change_1h < 0.0 {
        score -= 18;
        notes.push("Potensi blow-off: naik tajam tapi 1h mulai melemah.".to_string());
    } else if change_24h < MOMENTUM_BEARISH_24H_MIN && change_1h < 0.0 {
        score -= 20;
        notes.push("Trend turun berlanjut, sebaiknya hindari entry dulu.".to_string());
    } else if change_24h.abs() < MOMENTUM_FLAT_24H_MAX_ABS
        && change_1h.abs() < MOMENTUM_FLAT_1H_MAX_ABS
    {
        score -= 6;
        notes.push("Pergerakan datar, belum ada konfirmasi arah.".to_string());
    } else if change_1h >= 8.0 && change_24h.abs() < 5.0 {
        if change_5m >= MOMENTUM_5M_REBOUND_MIN {
            score += 6;
            notes.push("Spike 1h didukung akselerasi 5m (potensi breakout).".to_string());
        } else {
            score -= 6;
            notes.push("Spike 1h belum didukung struktur 24h, rawan fakeout.".to_string());
        }
    } else if change_5m <= MOMENTUM_5M_DUMP_MAX && change_1h > 0.0 {
        score -= 8;
        notes.push("Tekanan jual 5m muncul saat 1h hijau, perlu konfirmasi ulang.".to_string());
    }

    if (VOL_LIQ_RATIO_HEALTHY_MIN..=VOL_LIQ_RATIO_HEALTHY_MAX).contains(&vol_liq_ratio) {
        score += 8;
        notes.push("Rasio volume/likuiditas sehat untuk eksekusi.".to_string());
    } else if vol_liq_ratio > VOL_LIQ_RATIO_OVERHEATED_MIN {
        score -= 10;
        notes.push("Volume terlalu agresif vs likuiditas, rawan volatilitas ekstrem.".to_string());
    }

    if let Some(age_hours) = pair_age_hours(pair) {
        if age_hours < AGE_HOURS_VERY_NEW {
            score -= AGE_PENALTY_VERY_NEW;
            notes.push(
                "Pair sangat baru (<1 jam), risiko rug/manipulasi sangat tinggi.".to_string(),
            );
        } else if age_hours < AGE_HOURS_NEW {
            score -= AGE_PENALTY_NEW;
            notes.push("Pair baru (<6 jam), sebaiknya sangat konservatif.".to_string());
        } else if age_hours < AGE_HOURS_YOUNG {
            score -= AGE_PENALTY_YOUNG;
            notes.push("Pair masih muda (<24 jam), butuh konfirmasi tambahan.".to_string());
        } else if age_hours < AGE_HOURS_EARLY {
            score -= AGE_PENALTY_EARLY;
            notes.push("Pair belum matang (<7 hari), sizing sebaiknya kecil.".to_string());
        }
    }

    if !is_known_quote_symbol(pair) {
        score -= QUOTE_RISK_PENALTY;
        notes.push(
            "Quote token di luar whitelist (USDC/USDT/SOL), verifikasi manual disarankan."
                .to_string(),
        );
    }

    if let Some((buys, sells)) = weighted_pressure(pair) {
        let total = buys + sells;
        if total >= PRESSURE_MIN_CONFIDENCE_TXNS {
            if sells > buys * PRESSURE_DOMINANCE_RATIO {
                score += 4;
            } else if buys > sells * PRESSURE_DOMINANCE_RATIO {
                score -= 4;
            }
        }
    }

    score = score.clamp(0, 100);

    let signal = if score >= SCORE_GOOD_ENTRY_MIN {
        TradeSignal::GoodEntry
    } else if score >= SCORE_WATCHLIST_MIN {
        TradeSignal::Watchlist
    } else {
        TradeSignal::Avoid
    };

    if notes.is_empty() {
        notes.push("Data belum cukup untuk sinyal kuat.".to_string());
    }

    PairAssessment {
        signal,
        score,
        bias_summary,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{LiquidityData, PriceChange, TokenInfo, TxnSideCount, TxnsData, VolumeData};

    fn sample_pair(symbol: &str, volume: f64, liquidity: f64) -> PairData {
        PairData {
            chain_id: "solana".to_string(),
            dex_id: "raydium".to_string(),
            base_token: TokenInfo {
                name: Some(symbol.to_string()),
                symbol: Some(symbol.to_string()),
                address: Some("TokenMintAddress1111111111111111111111111".to_string()),
            },
            quote_token: TokenInfo {
                name: Some("USDC".to_string()),
                symbol: Some("USDC".to_string()),
                address: Some("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string()),
            },
            price_usd: Some("1.0".to_string()),
            price_change: Some(PriceChange {
                m5: Some(0.0),
                h1: Some(1.0),
                h6: Some(2.0),
                h24: Some(3.0),
            }),
            txns: Some(TxnsData {
                m5: Some(TxnSideCount {
                    buys: Some(30.0),
                    sells: Some(20.0),
                }),
                h1: Some(TxnSideCount {
                    buys: Some(120.0),
                    sells: Some(80.0),
                }),
                h24: Some(TxnSideCount {
                    buys: Some(500.0),
                    sells: Some(420.0),
                }),
            }),
            volume: Some(VolumeData {
                h24: Some(volume),
                h6: Some(volume / 2.0),
                h1: Some(volume / 4.0),
                m5: Some(volume / 10.0),
            }),
            liquidity: Some(LiquidityData {
                usd: Some(liquidity),
            }),
            fdv: Some(1000.0),
            pair_created_at: Some(0),
            pair_address: Some(format!("pair-{symbol}")),
            url: Some("https://dexscreener.com".to_string()),
        }
    }

    #[test]
    fn parse_usd_handles_invalid_input() {
        assert_eq!(parse_usd(&None), 0.0);
        assert_eq!(parse_usd(&Some("abc".to_string())), 0.0);
        assert_eq!(parse_usd(&Some("12.34".to_string())), 12.34);
    }

    #[test]
    fn apply_filters_respects_volume_and_liquidity() {
        let mut app = App::new();
        app.update_pairs(vec![
            sample_pair("AAA", 5_000.0, 2_000.0),
            sample_pair("BBB", 500.0, 2_000.0),
            sample_pair("CCC", 5_000.0, 100.0),
        ]);

        assert_eq!(app.filtered_pairs.len(), 1);
        let selected = app.selected_pair().expect("pair should exist");
        assert_eq!(selected.base_token.symbol.as_deref(), Some("AAA"));
    }

    #[test]
    fn assess_pair_returns_good_entry_on_healthy_momentum() {
        let mut pair = sample_pair("MOMO", 900_000.0, 300_000.0);
        if let Some(change) = pair.price_change.as_mut() {
            change.h24 = Some(12.0);
            change.h1 = Some(3.0);
            change.m5 = Some(1.5);
        }
        let assessed = assess_pair(&pair);
        assert_eq!(assessed.signal, TradeSignal::GoodEntry);
        assert!(assessed.score >= 67);
    }

    #[test]
    fn assess_pair_returns_avoid_on_weak_setup() {
        let mut pair = sample_pair("WEAK", 20_000.0, 8_000.0);
        if let Some(change) = pair.price_change.as_mut() {
            change.h24 = Some(-30.0);
            change.h1 = Some(-8.0);
        }
        let assessed = assess_pair(&pair);
        assert_eq!(assessed.signal, TradeSignal::Avoid);
        assert!(assessed.score < 45);
    }

    #[test]
    fn assess_pair_detects_blow_off_risk() {
        let mut pair = sample_pair("BLOW", 700_000.0, 120_000.0);
        if let Some(change) = pair.price_change.as_mut() {
            change.h24 = Some(72.0);
            change.h1 = Some(-2.0);
            change.m5 = Some(-3.0);
        }
        let assessed = assess_pair(&pair);
        assert!(assessed.notes.iter().any(|x| x.contains("blow-off")));
        assert!(assessed.score < 67);
    }

    #[test]
    fn assess_pair_detects_early_breakout() {
        let mut pair = sample_pair("BRK", 250_000.0, 90_000.0);
        if let Some(change) = pair.price_change.as_mut() {
            change.h24 = Some(2.0);
            change.h1 = Some(9.0);
            change.m5 = Some(2.5);
        }
        let assessed = assess_pair(&pair);
        assert!(assessed
            .notes
            .iter()
            .any(|x| x.contains("potensi breakout")));
    }

    #[test]
    fn assess_pair_handles_missing_values() {
        let pair = PairData {
            chain_id: "solana".to_string(),
            dex_id: "raydium".to_string(),
            base_token: TokenInfo {
                name: Some("EMPTY".to_string()),
                symbol: Some("EMP".to_string()),
                address: None,
            },
            quote_token: TokenInfo {
                name: Some("USDC".to_string()),
                symbol: Some("USDC".to_string()),
                address: None,
            },
            price_usd: None,
            price_change: None,
            txns: None,
            volume: None,
            liquidity: None,
            fdv: None,
            pair_created_at: None,
            pair_address: Some("emp".to_string()),
            url: None,
        };

        let assessed = assess_pair(&pair);
        assert!((0..=100).contains(&assessed.score));
        assert!(!assessed.notes.is_empty());
    }

    #[test]
    fn assess_pair_explains_sell_pressure_bias_to_base_token() {
        let mut pair = sample_pair("RAY", 400_000.0, 140_000.0);
        pair.quote_token.symbol = Some("SOL".to_string());
        pair.txns = Some(TxnsData {
            m5: Some(TxnSideCount {
                buys: Some(10.0),
                sells: Some(20.0),
            }),
            h1: Some(TxnSideCount {
                buys: Some(40.0),
                sells: Some(120.0),
            }),
            h24: None,
        });

        let assessed = assess_pair(&pair);
        assert!(assessed.bias_summary.contains("bias pilih RAY"));
    }

    #[test]
    fn assess_pair_explains_buy_pressure_bias_to_quote_token() {
        let mut pair = sample_pair("RAY", 400_000.0, 140_000.0);
        pair.quote_token.symbol = Some("SOL".to_string());
        pair.txns = Some(TxnsData {
            m5: Some(TxnSideCount {
                buys: Some(20.0),
                sells: Some(10.0),
            }),
            h1: Some(TxnSideCount {
                buys: Some(130.0),
                sells: Some(40.0),
            }),
            h24: None,
        });

        let assessed = assess_pair(&pair);
        assert!(assessed.bias_summary.contains("bias pilih SOL"));
    }

    #[test]
    fn assess_pair_penalizes_very_new_pair_age() {
        let mut pair = sample_pair("NEW", 700_000.0, 250_000.0);
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be valid")
            .as_millis() as u64;
        pair.pair_created_at = Some(now_ms.saturating_sub(30 * 60 * 1000)); // 30 min old

        let assessed = assess_pair(&pair);
        assert!(assessed
            .notes
            .iter()
            .any(|x| x.contains("Pair sangat baru")));
        assert!(assessed.score < 67);
    }

    #[test]
    fn assess_pair_flags_unknown_quote_token() {
        let mut pair = sample_pair("AAA", 400_000.0, 120_000.0);
        pair.quote_token.symbol = Some("XYZ".to_string());

        let assessed = assess_pair(&pair);
        assert!(assessed
            .notes
            .iter()
            .any(|x| x.contains("Quote token di luar whitelist")));
    }
}
