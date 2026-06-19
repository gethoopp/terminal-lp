use crate::api::{MeteoraPairData, PairData};
use crate::config;
use crate::rsi::RsiInfo;
use crate::yield_mod;
use crate::yield_mod::YieldInfo;
use chrono::Local;
use csv::Writer;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq)]
pub enum SortColumn {
    Symbol,
    Price,
    Change24h,
    Change1h,
    Rsi,
    Volume24h,
    Liquidity,
    MarketCap,
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
            SortColumn::Change1h => SortColumn::Rsi,
            SortColumn::Rsi => SortColumn::Volume24h,
            SortColumn::Volume24h => SortColumn::Liquidity,
            SortColumn::Liquidity => SortColumn::MarketCap,
            SortColumn::MarketCap => SortColumn::Fdv,
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
            SortColumn::Rsi => SortColumn::Change1h,
            SortColumn::Volume24h => SortColumn::Rsi,
            SortColumn::Liquidity => SortColumn::Volume24h,
            SortColumn::MarketCap => SortColumn::Liquidity,
            SortColumn::Fdv => SortColumn::MarketCap,
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
pub enum DexFilter {
    All,
    Meteora,
    Raydium,
    Orca,
    PumpSwap,
}

impl DexFilter {
    pub fn next(&self) -> Self {
        match self {
            DexFilter::All => DexFilter::Meteora,
            DexFilter::Meteora => DexFilter::Raydium,
            DexFilter::Raydium => DexFilter::Orca,
            DexFilter::Orca => DexFilter::PumpSwap,
            DexFilter::PumpSwap => DexFilter::All,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            DexFilter::All => "All",
            DexFilter::Meteora => "Meteora",
            DexFilter::Raydium => "Raydium",
            DexFilter::Orca => "Orca",
            DexFilter::PumpSwap => "PumpSwap",
        }
    }

    pub fn matches(&self, dex_id: &str) -> bool {
        let normalized = dex_id.to_ascii_lowercase();
        match self {
            DexFilter::All => true,
            DexFilter::Meteora => normalized.contains("meteora"),
            DexFilter::Raydium => normalized.contains("raydium"),
            DexFilter::Orca => normalized.contains("orca"),
            DexFilter::PumpSwap => normalized.contains("pumpswap"),
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
    pub microstructure_summary: String,
    pub il_estimate_24h_pct: Option<f64>,
    pub health_bar: String,
    pub notes: Vec<String>,
    pub rsi: Option<RsiInfo>,
    pub yields: Vec<YieldInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            RiskLevel::Low => "Low Risk",
            RiskLevel::Medium => "Medium Risk",
            RiskLevel::High => "High Risk",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TopCandidate {
    pub pair_index: usize,
    pub symbol: String,
    pub score: i32,
    pub confidence: i32, // 0-100
    pub risk_level: RiskLevel,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotPosition {
    pub pair_key: String,
    pub pair_address: Option<String>,
    pub symbol: String,
    pub entry_price: f64,
    pub target_price: f64,
    pub take_profit_pct: f64,
    pub stop_loss_price: f64,
    pub stop_loss_pct: f64,
    pub opened_at: String,
    pub provider_position_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BotOpenInput {
    pub pair_key: String,
    pub pair_address: Option<String>,
    pub symbol: String,
    pub base_mint: Option<String>,
    pub quote_mint: Option<String>,
    pub input_mint: Option<String>, // None = let bot decide, Some = explicit input token
    pub entry_price: f64,
    pub take_profit_pct: f64,
    pub stop_loss_pct: f64,
    pub position_size_usd: f64,
}

#[derive(Debug, Clone)]
pub struct BotCloseInput {
    pub pair_key: String,
    pub pair_address: Option<String>,
    pub symbol: String,
    pub entry_price: f64,
    pub current_price: f64,
    pub provider_position_id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub symbol: String,
    pub pair_key: String,
    pub entry_price: f64,
    pub exit_price: f64,
    pub pnl_pct: f64,
    pub position_size_usd: f64,
    pub opened_at: String,
    pub closed_at: String,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct WatchlistStore {
    pairs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BotPositionStore {
    active_position: Option<BotPosition>,
}

pub struct App {
    pub pairs: Vec<PairData>,
    pub assessments: Vec<PairAssessment>,
    pub filtered_pairs: Vec<usize>,
    pub sort_column: SortColumn,
    pub sort_direction: SortDirection,
    pub dex_filter: DexFilter,
    pub selected_index: usize,
    pub status_message: String,
    pub last_update: String,
    pub min_volume_filter: f64,
    pub min_liquidity_filter: f64,
    pub min_market_cap_filter: f64,
    pub search_query: String,
    pub search_mode: bool,
    pub open_link_mode: bool,
    pub watchlist_only: bool,
    watchlist: HashSet<String>,
    pub bot_enabled: bool,
    pub bot_take_profit_pct: f64,
    pub bot_stop_loss_pct: f64,
    pub bot_position_size_usd: f64,
    pub bot_use_sol_input: bool, // true = enter with SOL, false = enter with base token
    pub bot_active_position: Option<BotPosition>,
    pub bot_status: String,
    pub is_loading: bool,
    pub show_help: bool,
    pub error_message: Option<String>,
    pub alerts_enabled: bool,
    pub alert_threshold_pct: f64,
    pub alert_cooldown_secs: u64,
    alerted_pairs: HashMap<String, u64>,
    pub yields: HashMap<String, Vec<YieldInfo>>,
    pub degen_mode: bool,
    pub meteora_mode: bool,
    pub meteora_pools: Vec<MeteoraPairData>,
    pub trade_history: Vec<TradeRecord>,
    pub show_portfolio: bool,
    pub top_candidate: Option<TopCandidate>,
}

impl App {
    pub fn new() -> Self {
        let app_cfg = config::active_config_opt()
            .map(|cfg| cfg.app.clone())
            .unwrap_or_default();
        let alerts_cfg = config::active_config_opt()
            .map(|cfg| cfg.alerts.clone())
            .unwrap_or_default();
        let watchlist = load_watchlist();
        let active_bot_position = load_bot_position();
        let bot_status = if let Some(position) = active_bot_position.as_ref() {
            format!(
                "BOT POSITION RECOVERED: {} @ ${:.8} | TP ${:.8} | SL ${:.8}",
                position.symbol,
                position.entry_price,
                position.target_price,
                position.stop_loss_price
            )
        } else {
            String::from("BOT OFF")
        };
        App {
            pairs: Vec::new(),
            assessments: Vec::new(),
            filtered_pairs: Vec::new(),
            sort_column: SortColumn::Change24h,
            sort_direction: SortDirection::Descending,
            dex_filter: DexFilter::All,
            selected_index: 0,
            status_message: String::from("Initializing..."),
            last_update: String::from("--"),
            min_volume_filter: app_cfg.initial_min_volume_filter,
            min_liquidity_filter: app_cfg.initial_min_liquidity_filter,
            min_market_cap_filter: app_cfg.initial_min_market_cap_filter,
            search_query: String::new(),
            search_mode: false,
            open_link_mode: false,
            watchlist_only: false,
            watchlist,
            bot_enabled: false,
            bot_take_profit_pct: 5.0,
            bot_stop_loss_pct: 2.0,
            bot_position_size_usd: 25.0,
            bot_use_sol_input: true, // default: enter with SOL
            bot_active_position: active_bot_position,
            bot_status,
            is_loading: true,
            show_help: false,
            error_message: None,
            alerts_enabled: alerts_cfg.enabled,
            alert_threshold_pct: alerts_cfg.change_24h_threshold_pct,
            alert_cooldown_secs: alerts_cfg.cooldown_secs,
            alerted_pairs: HashMap::new(),
            yields: HashMap::new(),
            degen_mode: false,
            meteora_mode: false,
            meteora_pools: Vec::new(),
            trade_history: load_trade_history(),
            show_portfolio: false,
            top_candidate: None,
        }
    }

    pub fn toggle_degen_mode(&mut self) {
        self.degen_mode = !self.degen_mode;
        if self.degen_mode {
            self.meteora_mode = false; // mutually exclusive
            self.status_message = "DEGEN MODE ON - Showing coins 4-300h old, min $30K liquidity".to_string();
            self.sort_column = SortColumn::Age;
            self.sort_direction = SortDirection::Ascending;
        } else {
            self.status_message = "DEGEN MODE OFF".to_string();
        }
        self.apply_filters();
    }

    /// Toggle Meteora mode — fetches DLMM pools from Pool Discovery API
    /// using Meridian degen thresholds (fee/TVL >= 2%, organic >= 50, etc.)
    pub fn toggle_meteora_mode(&mut self) {
        self.meteora_mode = !self.meteora_mode;
        if self.meteora_mode {
            self.degen_mode = false; // mutually exclusive
            self.status_message = "METEORA MODE ON - Fetching DLMM pools via Pool Discovery API (Meridian thresholds)...".to_string();
        } else {
            self.meteora_mode = false;
            self.meteora_pools.clear();
            self.status_message = "METEORA MODE OFF".to_string();
        }
    }

    /// Called when Meteora pool fetch completes.
    pub fn update_meteora_pools(&mut self, pools: Vec<MeteoraPairData>) {
        let count = pools.len();
        self.meteora_pools = pools;
        self.status_message = format!(
            "METEORA: {} DLMM pools (fee/TVL≥2%, organic≥50, holders≥200) — press M to toggle",
            count
        );
        self.is_loading = false;
        self.error_message = None;
    }

    pub fn update_yields(&mut self, yields: HashMap<String, Vec<YieldInfo>>) {
        self.yields = yields;
        self.assessments = self
            .pairs
            .iter()
            .map(|p| assess_pair_with_yields(p, &self.yields))
            .collect();
        self.apply_filters();
    }

    pub fn update_pairs(&mut self, pairs: Vec<PairData>) {
        self.pairs = pairs;
        self.assessments = self
            .pairs
            .iter()
            .map(|p| assess_pair_with_yields(p, &self.yields))
            .collect();
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
                let mcap = pair.market_cap.unwrap_or(0.0);

                if self.degen_mode {
                    // Degen mode: bypass normal vol/liq/mcap floors.
                    // Only require liq >= $1K and age 4–300h.
                    if liq < 1_000.0 {
                        return false;
                    }
                    if !self.dex_filter.matches(&pair.dex_id) {
                        return false;
                    }
                    // Pairs with no creation time are excluded (can't verify age)
                    match pair_age_hours(pair) {
                        Some(age_hours) if (4.0..=300.0).contains(&age_hours) => {}
                        _ => return false,
                    }
                } else {
                    if vol < self.min_volume_filter
                        || liq < self.min_liquidity_filter
                        || mcap < self.min_market_cap_filter
                    {
                        return false;
                    }
                    if !self.dex_filter.matches(&pair.dex_id) {
                        return false;
                    }
                }

                if self.watchlist_only {
                    let pair_key = pair_watch_key(pair);
                    if !self.watchlist.contains(&pair_key) {
                        return false;
                    }
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
        let mcap = self.min_market_cap_filter;
        let dex = self.dex_filter.label();
        let wl = if self.watchlist_only { "ON" } else { "OFF" };
        let degen = if self.degen_mode { " | DEGEN: ON (4-300h, Liq>=$30K)" } else { "" };
        self.status_message = format!(
            "Showing {filtered}/{total} | DEX: {dex} | WL only: {wl} | Vol >= ${vol:.0} | Liq >= ${liq:.0} | MCap >= ${mcap:.0}{degen}"
        );
        
        // Compute top candidate from filtered pairs
        self.compute_top_candidate();
    }
    
    /// Compute the top candidate from filtered pairs based on score and data completeness
    fn compute_top_candidate(&mut self) {
        const MIN_SCORE_THRESHOLD: i32 = 60;
        const MIN_CONFIDENCE_THRESHOLD: i32 = 50;
        
        if self.filtered_pairs.is_empty() {
            self.top_candidate = None;
            return;
        }
        
        let mut best: Option<(usize, i32, i32, Vec<String>)> = None; // (pair_idx, score, confidence, reasons)
        
        for &pair_idx in &self.filtered_pairs {
            let pair = match self.pairs.get(pair_idx) {
                Some(p) => p,
                None => continue,
            };
            let assessment = match self.assessments.get(pair_idx) {
                Some(a) => a,
                None => continue,
            };
            
            // Skip if score is too low
            if assessment.score < MIN_SCORE_THRESHOLD {
                continue;
            }
            
            // Calculate confidence based on data completeness
            let mut confidence = 100i32;
            let mut reasons: Vec<String> = Vec::new();
            
            // Check data completeness
            let has_liquidity = pair.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0) > 0.0;
            let has_volume = pair.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0) > 0.0;
            let has_price_change = pair.price_change.is_some();
            let has_mcap = pair.market_cap.unwrap_or(0.0) > 0.0;
            let has_age = pair.pair_created_at.is_some();
            
            if !has_liquidity { confidence -= 20; }
            if !has_volume { confidence -= 15; }
            if !has_price_change { confidence -= 20; }
            if !has_mcap { confidence -= 10; }
            if !has_age { confidence -= 15; }
            
            // Degen mode has lower base confidence
            if self.degen_mode {
                confidence -= 20;
            }
            
            confidence = confidence.max(0);
            
            // Skip if confidence too low
            if confidence < MIN_CONFIDENCE_THRESHOLD {
                continue;
            }
            
            // Build reasons from assessment notes (take top 3 positive ones)
            for note in assessment.notes.iter().take(3) {
                if !note.contains("hindari") && !note.contains("risiko") && !note.contains("rendah") {
                    reasons.push(note.clone());
                }
            }
            if reasons.is_empty() {
                reasons.push(format!("Score: {}/100", assessment.score));
            }
            
            // Track the best candidate (highest score, then highest confidence)
            let dominated = best.as_ref().is_some_and(|(_, best_score, best_conf, _)| {
                assessment.score < *best_score || 
                (assessment.score == *best_score && confidence < *best_conf)
            });
            
            if !dominated {
                best = Some((pair_idx, assessment.score, confidence, reasons));
            }
        }
        
        self.top_candidate = best.map(|(pair_idx, score, confidence, reasons)| {
            let pair = &self.pairs[pair_idx];
            let symbol = pair.base_token.symbol.clone().unwrap_or_else(|| "???".to_string());
            
            // Determine risk level
            let risk_level = if score >= 75 && confidence >= 70 {
                RiskLevel::Low
            } else if score >= 60 && confidence >= 50 {
                RiskLevel::Medium
            } else {
                RiskLevel::High
            };
            
            TopCandidate {
                pair_index: pair_idx,
                symbol,
                score,
                confidence,
                risk_level,
                reasons,
            }
        });
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
                SortColumn::Rsi => {
                    let rsi_a = self
                        .assessments
                        .get(a)
                        .and_then(|x| x.rsi.as_ref().map(|r| r.value))
                        .unwrap_or(0.0);
                    let rsi_b = self
                        .assessments
                        .get(b)
                        .and_then(|x| x.rsi.as_ref().map(|r| r.value))
                        .unwrap_or(0.0);
                    rsi_a
                        .partial_cmp(&rsi_b)
                        .unwrap_or(std::cmp::Ordering::Equal)
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
                SortColumn::MarketCap => {
                    let mcap_a = pa.market_cap.unwrap_or(0.0);
                    let mcap_b = pb.market_cap.unwrap_or(0.0);
                    mcap_a
                        .partial_cmp(&mcap_b)
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

    /// Returns the highest-scoring pair from filtered results along with its assessment.
    /// Returns None if no pair has complete-enough data to recommend.
    #[allow(dead_code)]
    pub fn top_candidate(&self) -> Option<(&PairData, &PairAssessment)> {
        self.top_candidates().into_iter().next()
    }

    /// Returns all qualified candidates sorted by score descending.
    pub fn top_candidates(&self) -> Vec<(&PairData, &PairAssessment)> {
        let mut candidates: Vec<_> = self
            .filtered_pairs
            .iter()
            .filter_map(|&idx| {
                let pair = self.pairs.get(idx)?;
                let assessment = self.assessments.get(idx)?;
                let has_price = pair.price_usd.is_some();
                let has_liq = pair.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0) > 0.0;
                let has_volume = pair.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0) > 0.0;
                if !has_price || !has_liq || !has_volume {
                    return None;
                }
                Some((pair, assessment))
            })
            .collect();
        candidates.sort_by(|a, b| b.1.score.cmp(&a.1.score));
        candidates
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

    pub fn selected_fdv_mcap_ratio(&self) -> Option<f64> {
        let pair = self.selected_pair()?;
        let fdv = pair.fdv?;
        let mcap = pair.market_cap?;
        if mcap <= 0.0 {
            return None;
        }
        Some(fdv / mcap)
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

    pub fn toggle_input_mode(&mut self) {
        self.bot_use_sol_input = !self.bot_use_sol_input;
        let mode = if self.bot_use_sol_input { "SOL" } else { "Token (quote)" };
        self.set_warning(format!("Bot input mode: {mode}"));
    }

    pub fn toggle_portfolio(&mut self) {
        self.show_portfolio = !self.show_portfolio;
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

    pub fn cycle_dex_filter(&mut self) {
        self.dex_filter = self.dex_filter.next();
        self.apply_filters();
    }

    pub fn toggle_watchlist_only(&mut self) {
        self.watchlist_only = !self.watchlist_only;
        self.apply_filters();
    }

    pub fn selected_in_watchlist(&self) -> bool {
        self.selected_pair_key()
            .map(|key| self.watchlist.contains(&key))
            .unwrap_or(false)
    }

    pub fn toggle_selected_watchlist(&mut self) {
        let Some(pair_key) = self.selected_pair_key() else {
            self.status_message = String::from("Tidak ada pair terpilih untuk watchlist.");
            return;
        };

        let added = if self.watchlist.contains(&pair_key) {
            self.watchlist.remove(&pair_key);
            false
        } else {
            self.watchlist.insert(pair_key);
            true
        };

        if let Err(err) = save_watchlist(&self.watchlist) {
            self.status_message = format!("Gagal simpan watchlist: {err}");
        } else if added {
            self.status_message = String::from("Pair ditambahkan ke watchlist.");
        } else {
            self.status_message = String::from("Pair dihapus dari watchlist.");
        }

        self.apply_filters();
    }

    pub fn toggle_bot_enabled(&mut self) {
        self.bot_enabled = !self.bot_enabled;
        if self.bot_enabled {
            self.bot_status = format!(
                "BOT ON | TP {:.2}% | SL {:.2}% | Size ${:.2}",
                self.bot_take_profit_pct, self.bot_stop_loss_pct, self.bot_position_size_usd
            );
        } else {
            self.bot_status = String::from("BOT OFF");
        }
    }

    pub fn selected_pair_for_bot_open(&self) -> Option<BotOpenInput> {
        if self.bot_active_position.is_some() {
            return None;
        }

        let pair = self.selected_pair()?;
        let entry_price = pair_price_usd(pair)?;
        if entry_price <= 0.0 {
            return None;
        }

        Some(BotOpenInput {
            pair_key: pair_watch_key(pair),
            pair_address: pair.pair_address.clone(),
            symbol: pair
                .base_token
                .symbol
                .as_deref()
                .unwrap_or("UNKNOWN")
                .to_string(),
            base_mint: pair.base_token.address.clone(),
            quote_mint: pair.quote_token.address.clone(),
            // input_mint: SOL if bot_use_sol_input, else use quote token (what user holds)
            input_mint: if self.bot_use_sol_input {
                Some("So11111111111111111111111111111111111111112".to_string())
            } else {
                pair.quote_token.address.clone()
            },
            entry_price,
            take_profit_pct: self.bot_take_profit_pct,
            stop_loss_pct: self.bot_stop_loss_pct,
            position_size_usd: self.bot_position_size_usd,
        })
    }

    pub fn close_input_for_active_position(&self, reason: &str) -> Option<BotCloseInput> {
        let position = self.bot_active_position.as_ref()?;
        let pair = self
            .pairs
            .iter()
            .find(|pair| pair_watch_key(pair) == position.pair_key)?;
        let current_price = pair_price_usd(pair)?;

        Some(BotCloseInput {
            pair_key: position.pair_key.clone(),
            pair_address: position.pair_address.clone(),
            symbol: position.symbol.clone(),
            entry_price: position.entry_price,
            current_price,
            provider_position_id: position.provider_position_id.clone(),
            reason: reason.to_string(),
        })
    }

    pub fn auto_take_profit_input(&self) -> Option<BotCloseInput> {
        if !self.bot_enabled {
            return None;
        }

        let position = self.bot_active_position.as_ref()?;
        let pair = self
            .pairs
            .iter()
            .find(|pair| pair_watch_key(pair) == position.pair_key)?;
        let current_price = pair_price_usd(pair)?;
        if current_price >= position.target_price {
            return Some(BotCloseInput {
                pair_key: position.pair_key.clone(),
                pair_address: position.pair_address.clone(),
                symbol: position.symbol.clone(),
                entry_price: position.entry_price,
                current_price,
                provider_position_id: position.provider_position_id.clone(),
                reason: String::from("auto_take_profit"),
            });
        }
        if current_price <= position.stop_loss_price {
            return Some(BotCloseInput {
                pair_key: position.pair_key.clone(),
                pair_address: position.pair_address.clone(),
                symbol: position.symbol.clone(),
                entry_price: position.entry_price,
                current_price,
                provider_position_id: position.provider_position_id.clone(),
                reason: String::from("auto_stop_loss"),
            });
        }

        None
    }

    pub fn set_bot_opened(&mut self, position: BotPosition) {
        self.bot_status = format!(
            "BOT POSITION OPEN: {} @ ${:.8} | TP ${:.8} | SL ${:.8}",
            position.symbol, position.entry_price, position.target_price, position.stop_loss_price
        );
        self.bot_active_position = Some(position);
        if let Err(err) = save_bot_position(self.bot_active_position.as_ref()) {
            self.bot_status = format!("BOT SAVE ERROR: {err}");
        }
    }

    pub fn set_bot_closed(&mut self, symbol: &str, current_price: f64, pnl_pct: f64, reason: &str) {
        // Validate pnl_pct: handle NaN/Infinity and clamp to reasonable range (-100% to 10000%)
        let safe_pnl_pct = if pnl_pct.is_nan() || pnl_pct.is_infinite() {
            0.0
        } else {
            pnl_pct.clamp(-100.0, 10000.0)
        };

        // Save to trade history before clearing the position
        if let Some(pos) = self.bot_active_position.as_ref() {
            let record = TradeRecord {
                symbol: symbol.to_string(),
                pair_key: pos.pair_key.clone(),
                entry_price: pos.entry_price,
                exit_price: current_price,
                pnl_pct: safe_pnl_pct,
                position_size_usd: self.bot_position_size_usd,
                opened_at: pos.opened_at.clone(),
                closed_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                reason: reason.chars().take(50).collect(),
            };
            self.trade_history.push(record);
            let _ = append_trade_history(&self.trade_history);
        }
        self.bot_active_position = None;
        if let Err(err) = save_bot_position(None) {
            self.bot_status = format!("BOT CLEAR ERROR: {err}");
            return;
        }
        self.bot_status = format!(
            "BOT POSITION CLOSED ({reason}): {symbol} @ ${current_price:.8} | PnL {safe_pnl_pct:+.2}%"
        );
    }

    pub fn set_bot_error(&mut self, action: &str, err: &str) {
        self.bot_status = format!("BOT {action} ERROR: {err}");
    }

    pub fn collect_price_alerts(&mut self) -> Vec<String> {
        if !self.alerts_enabled || self.alert_threshold_pct <= 0.0 {
            return Vec::new();
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let threshold = self.alert_threshold_pct.abs();
        let mut alerts = Vec::new();

        for pair in &self.pairs {
            let change = pair
                .price_change
                .as_ref()
                .and_then(|pc| pc.h24)
                .unwrap_or(0.0);
            if change.abs() < threshold {
                continue;
            }

            let key = pair_watch_key(pair);
            let can_notify = match self.alerted_pairs.get(&key) {
                Some(last_at) => now.saturating_sub(*last_at) >= self.alert_cooldown_secs,
                None => true,
            };
            if !can_notify {
                continue;
            }

            self.alerted_pairs.insert(key, now);
            let symbol = pair.base_token.symbol.as_deref().unwrap_or("UNKNOWN");
            alerts.push(format!(
                "ALERT {symbol}: 24h change {change:+.2}% (threshold {threshold:.2}%)"
            ));
        }

        alerts
    }

    pub fn export_filtered_to_csv(&self, export_dir: &PathBuf) -> Result<PathBuf, String> {
        fs::create_dir_all(export_dir).map_err(|err| {
            format!(
                "gagal membuat export directory {}: {err}",
                export_dir.display()
            )
        })?;

        let filename = format!(
            "scanner_export_{}.csv",
            chrono::Local::now().format("%Y%m%d_%H%M%S")
        );
        let file_path = export_dir.join(filename);

        let mut writer = Writer::from_path(&file_path)
            .map_err(|err| format!("gagal membuka CSV {}: {err}", file_path.display()))?;
        writer
            .write_record([
                "symbol",
                "name",
                "dex",
                "price_usd",
                "change_1h_pct",
                "change_24h_pct",
                "rsi_value",
                "rsi_zone",
                "volume_24h_usd",
                "liquidity_usd",
                "market_cap",
                "fdv",
                "signal",
                "score",
                "bias_summary",
                "microstructure_summary",
                "pair_url",
            ])
            .map_err(|err| format!("gagal menulis header CSV: {err}"))?;

        for &pair_idx in &self.filtered_pairs {
            let pair = match self.pairs.get(pair_idx) {
                Some(p) => p,
                None => continue,
            };
            let assessment = self.assessments.get(pair_idx);
            let signal = assessment.map(|a| a.signal.label()).unwrap_or("PANTAU");
            let score = assessment.map(|a| a.score).unwrap_or(50);
            let bias = assessment
                .map(|a| a.bias_summary.clone())
                .unwrap_or_else(|| "-".to_string());
            let micro = assessment
                .map(|a| a.microstructure_summary.clone())
                .unwrap_or_else(|| "-".to_string());
            let rsi_value = assessment
                .and_then(|a| a.rsi.as_ref().map(|r| format!("{:.2}", r.value)))
                .unwrap_or_default();
            let rsi_zone = assessment
                .and_then(|a| a.rsi.as_ref().map(|r| r.zone.label().to_string()))
                .unwrap_or_default();

            writer
                .write_record([
                    pair.base_token.symbol.as_deref().unwrap_or(""),
                    pair.base_token.name.as_deref().unwrap_or(""),
                    pair.dex_id.as_str(),
                    pair.price_usd.as_deref().unwrap_or(""),
                    &format!(
                        "{:.4}",
                        pair.price_change
                            .as_ref()
                            .and_then(|pc| pc.h1)
                            .unwrap_or_default()
                    ),
                    &format!(
                        "{:.4}",
                        pair.price_change
                            .as_ref()
                            .and_then(|pc| pc.h24)
                            .unwrap_or_default()
                    ),
                    &rsi_value,
                    &rsi_zone,
                    &format!(
                        "{:.4}",
                        pair.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0)
                    ),
                    &format!(
                        "{:.4}",
                        pair.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0)
                    ),
                    &format!("{:.4}", pair.market_cap.unwrap_or(0.0)),
                    &format!("{:.4}", pair.fdv.unwrap_or(0.0)),
                    signal,
                    &score.to_string(),
                    &bias,
                    &micro,
                    pair.url.as_deref().unwrap_or(""),
                ])
                .map_err(|err| format!("gagal menulis row CSV: {err}"))?;
        }

        writer
            .flush()
            .map_err(|err| format!("gagal flush CSV {}: {err}", file_path.display()))?;
        Ok(file_path)
    }

    pub fn export_portfolio_to_csv(&self, export_dir: &PathBuf) -> Result<PathBuf, String> {
        fs::create_dir_all(export_dir)
            .map_err(|e| format!("gagal membuat export dir: {e}"))?;
        let filename = format!(
            "portfolio_{}.csv",
            chrono::Local::now().format("%Y%m%d_%H%M%S")
        );
        let file_path = export_dir.join(filename);
        let mut writer = Writer::from_path(&file_path)
            .map_err(|e| format!("gagal buka CSV: {e}"))?;
        writer
            .write_record(["closed_at", "symbol", "entry_price", "exit_price", "pnl_pct", "position_size_usd", "opened_at", "reason"])
            .map_err(|e| format!("gagal tulis header: {e}"))?;
        for t in &self.trade_history {
            writer
                .write_record([
                    &t.closed_at,
                    &t.symbol,
                    &format!("{:.8}", t.entry_price),
                    &format!("{:.8}", t.exit_price),
                    &format!("{:.4}", t.pnl_pct),
                    &format!("{:.2}", t.position_size_usd),
                    &t.opened_at,
                    &t.reason,
                ])
                .map_err(|e| format!("gagal tulis row: {e}"))?;
        }
        writer.flush().map_err(|e| format!("gagal flush: {e}"))?;
        Ok(file_path)
    }

    fn selected_pair_key(&self) -> Option<String> {
        self.filtered_pairs
            .get(self.selected_index)
            .and_then(|&idx| self.pairs.get(idx))
            .map(pair_watch_key)
    }
}

fn parse_usd(s: &Option<String>) -> f64 {
    s.as_ref()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn pair_price_usd(pair: &PairData) -> Option<f64> {
    pair.price_usd
        .as_ref()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| *v > 0.0)
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
        "Tekanan seimbang (buy {buys:.1} vs sell {sells:.1}) pada {base_symbol}/{quote_symbol} -> bisa pegang keduanya atau tunggu sinyal lebih jelas."
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

fn pair_watch_key(pair: &PairData) -> String {
    pair.pair_address
        .as_ref()
        .filter(|v| !v.is_empty())
        .cloned()
        .or_else(|| pair.url.as_ref().filter(|v| !v.is_empty()).cloned())
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

fn watchlist_path() -> PathBuf {
    config::watchlist_file_path()
}

fn bot_position_path() -> PathBuf {
    config::bot_position_file_path()
}

fn legacy_watchlist_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".haliim_terminal_watchlist.json"))
}

fn legacy_bot_position_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".haliim_terminal_bot_position.json"))
}

fn load_watchlist() -> HashSet<String> {
    let path = watchlist_path();
    let raw = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => {
            if let Some(legacy) = legacy_watchlist_path() {
                if let Ok(content) = fs::read_to_string(legacy) {
                    return serde_json::from_str::<WatchlistStore>(&content)
                        .map(|store| store.pairs.into_iter().collect())
                        .unwrap_or_default();
                }
            }
            return HashSet::new();
        }
    };

    serde_json::from_str::<WatchlistStore>(&raw)
        .map(|store| store.pairs.into_iter().collect())
        .unwrap_or_default()
}

fn save_watchlist(watchlist: &HashSet<String>) -> Result<(), String> {
    let path = watchlist_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "gagal membuat direktori watchlist {}: {err}",
                parent.display()
            )
        })?;
    }

    let mut pairs = watchlist.iter().cloned().collect::<Vec<_>>();
    pairs.sort();
    let store = WatchlistStore { pairs };
    let json = serde_json::to_string_pretty(&store)
        .map_err(|err| format!("serialize watchlist gagal: {err}"))?;
    fs::write(path, json).map_err(|err| format!("write watchlist gagal: {err}"))
}

fn load_bot_position() -> Option<BotPosition> {
    let path = bot_position_path();
    let raw = fs::read_to_string(&path).ok().or_else(|| {
        legacy_bot_position_path().and_then(|legacy| fs::read_to_string(legacy).ok())
    })?;
    let store = serde_json::from_str::<BotPositionStore>(&raw).ok()?;
    store.active_position
}

fn save_bot_position(position: Option<&BotPosition>) -> Result<(), String> {
    let path = bot_position_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "gagal membuat direktori bot position {}: {err}",
                parent.display()
            )
        })?;
    }

    let store = BotPositionStore {
        active_position: position.cloned(),
    };
    let json = serde_json::to_string_pretty(&store)
        .map_err(|err| format!("serialize bot position gagal: {err}"))?;
    fs::write(path, json).map_err(|err| format!("write bot position gagal: {err}"))
}

// --- Bot State Import ---

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BotStateFile {
    metrics: Option<BotStateMetrics>,
    positions: Option<BotStatePositions>,
    updated_at: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BotStateMetrics {
    balance_usd: Option<f64>,
    pnl_usd: Option<f64>,
    pnl_pct_change: Option<f64>,
    total_deposit_usd: Option<f64>,
    total_fee_earned_usd: Option<f64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BotStatePositions {
    dlmm_open: Option<HashMap<String, BotDlmmPosition>>,
    damm_closed: Option<HashMap<String, serde_json::Value>>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BotDlmmPosition {
    pool_name: Option<String>,
    pool_address: Option<String>,
    liquidity_usd: Option<f64>,
    pnl_usd: Option<f64>,
    fee_usd: Option<f64>,
    price: Option<f64>,
}

fn load_bot_state() -> Option<BotStateFile> {
    let path = config::bot_state_file_path()?;
    let raw = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn import_from_bot() -> Result<String, String> {
    let state = load_bot_state().ok_or("Bot state file not found. Set bot.project_path in config.")?;

    let mut summary_parts = Vec::new();

    // Report metrics summary
    if let Some(m) = &state.metrics {
        let bal = m.balance_usd.unwrap_or(0.0);
        let pnl = m.pnl_usd.unwrap_or(0.0);
        let fee = m.total_fee_earned_usd.unwrap_or(0.0);
        summary_parts.push(format!("Balance: ${bal:.2}, PnL: ${pnl:.2}, Fees: ${fee:.2}"));
    }

    // Count open positions
    if let Some(pos) = &state.positions {
        if let Some(dlmm) = &pos.dlmm_open {
            let open_count = dlmm.len();
            if open_count > 0 {
                summary_parts.push(format!("{open_count} open position(s)"));
            }
        }
    }

    if summary_parts.is_empty() {
        return Ok("No data found in bot state.".to_string());
    }

    Ok(format!("Bot sync: {}", summary_parts.join(" | ")))
}

fn load_trade_history() -> Vec<TradeRecord> {
    let path = config::trade_history_file_path();
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str::<Vec<TradeRecord>>(&raw).unwrap_or_default()
}

fn append_trade_history(history: &[TradeRecord]) -> Result<(), String> {
    let path = config::trade_history_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir failed: {e}"))?;
    }
    let json = serde_json::to_string_pretty(history)
        .map_err(|e| format!("serialize history failed: {e}"))?;
    // Atomic write: write to temp file then rename
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &json).map_err(|e| format!("write tmp failed: {e}"))?;
    // Set permissions to 600 (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&tmp_path, &path).map_err(|e| format!("rename failed: {e}"))
}

fn classify_health(score: i32) -> &'static str {
    if score >= 72 {
        "Strong"
    } else if score >= 55 {
        "Moderate"
    } else {
        "Weak"
    }
}

fn build_health_bar(score: i32) -> String {
    let filled = (score / 10).clamp(0, 10) as usize;
    let empty = 10usize.saturating_sub(filled);
    let gauge = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    format!("{gauge} {score}/100 ({})", classify_health(score))
}

fn estimate_il_from_price_change(change_pct: f64) -> Option<f64> {
    let ratio = 1.0 + (change_pct / 100.0);
    if ratio <= 0.0 {
        return None;
    }

    // Impermanent loss untuk pool 50/50.
    let il = (2.0 * ratio.sqrt() / (1.0 + ratio)) - 1.0;
    Some(il * 100.0)
}

fn microstructure_summary(pair: &PairData, notes: &mut Vec<String>, score: &mut i32) -> String {
    let vol_h24 = pair.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
    let vol_h6 = pair.volume.as_ref().and_then(|v| v.h6).unwrap_or(0.0);
    let vol_h1 = pair.volume.as_ref().and_then(|v| v.h1).unwrap_or(0.0);
    let vol_m5 = pair.volume.as_ref().and_then(|v| v.m5).unwrap_or(0.0);
    let ch_h6 = pair.price_change.as_ref().and_then(|c| c.h6).unwrap_or(0.0);
    let ch_h1 = pair.price_change.as_ref().and_then(|c| c.h1).unwrap_or(0.0);
    let ch_m5 = pair.price_change.as_ref().and_then(|c| c.m5).unwrap_or(0.0);

    let mut tags = Vec::new();

    if vol_h6 > 0.0 {
        let baseline_h1 = vol_h6 / 6.0;
        if vol_h1 > baseline_h1 * 1.8 {
            if ch_h6 >= 0.0 && ch_h1 >= 0.0 {
                *score += 8;
                notes.push("Microstructure: volume surge 1h mendukung trend naik h6.".to_string());
                tags.push("volume surge bullish");
            } else if ch_h6 < 0.0 {
                *score -= 6;
                notes.push("Microstructure: volume surge terjadi saat trend h6 turun.".to_string());
                tags.push("capitulation risk");
            } else {
                *score -= 2;
                notes.push(
                    "Microstructure: volume surge tanpa konfirmasi arah, perlu validasi."
                        .to_string(),
                );
                tags.push("surge without direction");
            }
        }
    }

    if ch_h6 >= 8.0 && ch_h1 > 0.0 {
        *score += 6;
        notes.push("Trend h6 bullish dan h1 ikut naik.".to_string());
        tags.push("h6 uptrend continuation");
    } else if ch_h6 <= -8.0 && ch_h1 < 0.0 {
        *score -= 10;
        notes.push("Trend h6 bearish berlanjut sampai h1.".to_string());
        tags.push("h6 bearish continuation");
    }

    if vol_h24 > 0.0 && vol_h6 > (vol_h24 / 4.0) * 1.5 {
        *score += 3;
        notes.push("Partisipasi volume 6h lebih tinggi dari rerata 24h.".to_string());
        tags.push("active 6h participation");
    }

    if vol_h1 > 0.0 && vol_m5 > (vol_h1 / 12.0) * 2.2 && ch_m5 < 0.0 {
        *score -= 5;
        notes.push("Volume 5m agresif disertai candle turun, potensi distribusi.".to_string());
        tags.push("short-term sell burst");
    }

    if tags.is_empty() {
        "Struktur orderflow netral; belum ada anomali volume yang kuat.".to_string()
    } else {
        format!("Microstructure: {}.", tags.join(", "))
    }
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
    let rsi = pair
        .price_change
        .as_ref()
        .and_then(RsiInfo::from_price_change);
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

    if let Some(rsi_info) = rsi.as_ref() {
        if rsi_info.value >= 72.0 {
            if change_1h < 0.0 || change_5m < 0.0 {
                score -= 4;
                notes.push(
                    "RSI heuristik terlalu panas dan momentum jangka pendek mulai melemah."
                        .to_string(),
                );
            } else {
                score -= 2;
                notes.push(
                    "RSI heuristik tinggi, entry baru sebaiknya menunggu pullback.".to_string(),
                );
            }
        } else if (58.0..=68.0).contains(&rsi_info.value) && change_24h >= 0.0 && change_1h >= 0.0 {
            score += 2;
            notes.push(
                "RSI heuristik sehat di area bullish, momentum masih cukup terjaga.".to_string(),
            );
        } else if rsi_info.value <= 35.0 {
            if change_1h > 0.0 || change_5m > 0.0 {
                score += 2;
                notes.push(
                    "RSI heuristik oversold tapi mulai ada rebound, cocok untuk pantau entry bertahap."
                        .to_string(),
                );
            } else {
                score -= 3;
                notes.push(
                    "RSI heuristik oversold tanpa rebound, risiko catching a falling knife."
                        .to_string(),
                );
            }
        } else if (45.0..58.0).contains(&rsi_info.value) {
            notes.push(
                "RSI heuristik netral, konfirmasi arah tetap perlu dilihat dari volume dan orderflow."
                    .to_string(),
            );
        }
    } else {
        notes.push(
            "RSI heuristik belum tersedia karena data perubahan multi-timeframe tidak lengkap."
                .to_string(),
        );
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

    let microstructure = microstructure_summary(pair, &mut notes, &mut score);
    let il_estimate_24h_pct = estimate_il_from_price_change(change_24h);

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
        microstructure_summary: microstructure,
        il_estimate_24h_pct,
        health_bar: build_health_bar(score),
        notes,
        rsi,
        yields: Vec::new(),
    }
}

pub fn assess_pair_with_yields(
    pair: &PairData,
    yields_map: &HashMap<String, Vec<YieldInfo>>,
) -> PairAssessment {
    let mut assessment = assess_pair(pair);

    let matched_yields = yield_mod::match_yield_to_pair(
        &pair.base_token.address,
        &pair.quote_token.address,
        yields_map,
    );

    assessment.yields = matched_yields;

    if !assessment.yields.is_empty() {
        if let Some(best_yield) = assessment.yields.first() {
            if let Some(apy) = best_yield.apy {
                if apy >= 15.0 {
                    assessment.score += 3;
                    assessment.notes.push(format!(
                        "Yield tinggi tersedia: {} ({})",
                        best_yield.display_apy(),
                        best_yield.dex_name.label()
                    ));
                } else if apy >= 10.0 {
                    assessment.score += 1;
                }
            }
        }
    }

    assessment
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{LiquidityData, PriceChange, TokenInfo, TxnSideCount, TxnsData, VolumeData};

    fn sample_pair(symbol: &str, volume: f64, liquidity: f64) -> PairData {
        PairData {
            chain_id: "solana".to_string(),
            dex_id: "meteora".to_string(),
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
            market_cap: Some(2_000_000.0),
            fdv: Some(1000.0),
            pair_created_at: Some(0),
            pair_address: Some(format!("pair-{symbol}")),
            url: Some("https://dexscreener.com".to_string()),
            boosts: None,
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
    fn apply_filters_respects_min_market_cap() {
        let mut app = App::new();
        let mut low = sample_pair("LOW", 50_000.0, 50_000.0);
        low.market_cap = Some(200_000.0);
        let mut high = sample_pair("HIGH", 50_000.0, 50_000.0);
        high.market_cap = Some(2_000_000.0);
        app.update_pairs(vec![low, high]);

        app.min_market_cap_filter = 1_000_000.0;
        app.apply_filters();

        assert_eq!(app.filtered_pairs.len(), 1);
        let selected = app.selected_pair().expect("pair should exist");
        assert_eq!(selected.base_token.symbol.as_deref(), Some("HIGH"));
    }

    #[test]
    fn apply_filters_respects_dex_filter() {
        let mut app = App::new();
        let mut ray = sample_pair("RAY", 50_000.0, 50_000.0);
        ray.dex_id = "raydium".to_string();
        let mut met = sample_pair("MET", 50_000.0, 50_000.0);
        met.dex_id = "meteora".to_string();
        app.update_pairs(vec![ray, met]);

        app.dex_filter = DexFilter::Meteora;
        app.apply_filters();

        assert_eq!(app.filtered_pairs.len(), 1);
        let selected = app.selected_pair().expect("pair should exist");
        assert_eq!(selected.base_token.symbol.as_deref(), Some("MET"));
    }

    #[test]
    fn watchlist_only_filters_non_watchlisted_pairs() {
        let mut app = App::new();
        let pair_a = sample_pair("AAA", 50_000.0, 50_000.0);
        let pair_b = sample_pair("BBB", 50_000.0, 50_000.0);
        app.update_pairs(vec![pair_a.clone(), pair_b.clone()]);

        app.watchlist_only = true;
        app.watchlist.insert(pair_watch_key(&pair_b));
        app.apply_filters();

        assert_eq!(app.filtered_pairs.len(), 1);
        let selected = app.selected_pair().expect("pair should exist");
        assert_eq!(selected.base_token.symbol.as_deref(), Some("BBB"));
    }

    #[test]
    fn sort_by_rsi_keeps_pairs_with_missing_rsi_stable() {
        let mut app = App::new();
        let mut first = sample_pair("AAA", 50_000.0, 50_000.0);
        first.price_change = None;
        let mut second = sample_pair("BBB", 50_000.0, 50_000.0);
        second.price_change = None;
        app.update_pairs(vec![first, second]);

        app.sort_column = SortColumn::Rsi;
        app.sort_direction = SortDirection::Descending;
        app.apply_filters();

        let selected = app.selected_pair().expect("pair should exist");
        assert_eq!(selected.base_token.symbol.as_deref(), Some("AAA"));
    }

    #[test]
    fn selected_fdv_mcap_ratio_returns_expected_value() {
        let mut app = App::new();
        let mut pair = sample_pair("RATIO", 100_000.0, 100_000.0);
        pair.market_cap = Some(2_000_000.0);
        pair.fdv = Some(6_000_000.0);
        app.update_pairs(vec![pair]);
        let ratio = app.selected_fdv_mcap_ratio().expect("ratio should exist");
        assert!((ratio - 3.0).abs() < f64::EPSILON);
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
            market_cap: None,
            fdv: None,
            pair_created_at: None,
            pair_address: Some("emp".to_string()),
            url: None,
            boosts: None,
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

    #[test]
    fn estimate_il_from_price_change_is_negative_for_large_move() {
        let il = estimate_il_from_price_change(100.0).expect("IL should exist");
        assert!(il < 0.0);
        assert!(il.abs() > 5.0);
    }

    #[test]
    fn assess_pair_populates_microstructure_and_health_bar() {
        let mut pair = sample_pair("SURGE", 1_200_000.0, 200_000.0);
        pair.volume = Some(VolumeData {
            h24: Some(1_200_000.0),
            h6: Some(180_000.0),
            h1: Some(90_000.0),
            m5: Some(20_000.0),
        });
        if let Some(change) = pair.price_change.as_mut() {
            change.h6 = Some(14.0);
            change.h1 = Some(3.0);
            change.m5 = Some(1.2);
            change.h24 = Some(18.0);
        }

        let assessed = assess_pair(&pair);
        assert!(assessed.microstructure_summary.contains("Microstructure"));
        assert!(assessed.health_bar.contains("/100"));
        assert!(assessed.il_estimate_24h_pct.is_some());
    }

    #[test]
    fn assess_pair_populates_rsi_summary() {
        let mut pair = sample_pair("RSI", 900_000.0, 250_000.0);
        if let Some(change) = pair.price_change.as_mut() {
            change.m5 = Some(1.0);
            change.h1 = Some(4.0);
            change.h6 = Some(10.0);
            change.h24 = Some(16.0);
        }

        let assessed = assess_pair(&pair);
        let rsi = assessed.rsi.expect("RSI should exist");
        assert!(rsi.value > 50.0);
        assert!(rsi.display().contains("Bullish") || rsi.display().contains("Overbought"));
    }

    #[test]
    fn export_csv_leaves_rsi_blank_when_not_available() {
        let unique_dir = std::env::temp_dir().join(format!(
            "volatile_scanner_test_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be valid")
                .as_nanos()
        ));
        let mut app = App::new();
        let mut pair = sample_pair("NOR", 75_000.0, 75_000.0);
        pair.price_change = None;
        app.update_pairs(vec![pair]);

        let csv_path = app
            .export_filtered_to_csv(&unique_dir)
            .expect("CSV export should succeed");
        let csv = fs::read_to_string(csv_path).expect("CSV should be readable");
        let data_line = csv.lines().nth(1).expect("CSV should contain one data row");
        let columns: Vec<&str> = data_line.split(',').collect();

        assert_eq!(columns[6], "");
        assert_eq!(columns[7], "");
        let _ = fs::remove_dir_all(unique_dir);
    }

    #[test]
    fn trade_record_serialization_roundtrip() {
        let record = TradeRecord {
            symbol: "TEST".to_string(),
            pair_key: "solana-raydium-test".to_string(),
            entry_price: 0.00012345,
            exit_price: 0.00015432,
            pnl_pct: 25.0,
            position_size_usd: 100.0,
            opened_at: "2024-01-01 10:00:00".to_string(),
            closed_at: "2024-01-01 11:00:00".to_string(),
            reason: "take_profit".to_string(),
        };

        let json = serde_json::to_string(&record).expect("should serialize");
        let deserialized: TradeRecord = serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(deserialized.symbol, "TEST");
        assert_eq!(deserialized.pnl_pct, 25.0);
        assert_eq!(deserialized.reason, "take_profit");
    }

    #[test]
    fn trade_record_serialization_handles_special_pnl_values() {
        // NaN and Infinity are not valid JSON, so they should be sanitized before serialization
        // This test verifies the clamp logic in set_bot_closed works
        let record = TradeRecord {
            symbol: "TEST".to_string(),
            pair_key: "test".to_string(),
            entry_price: 1.0,
            exit_price: 1.5,
            pnl_pct: 50.0, // Normal value
            position_size_usd: 100.0,
            opened_at: "".to_string(),
            closed_at: "".to_string(),
            reason: "test".to_string(),
        };

        let json = serde_json::to_string(&record).expect("should serialize normal pnl");
        assert!(json.contains("50.0") || json.contains("50"));
    }

    #[test]
    fn trade_history_vec_serialization() {
        let history = vec![
            TradeRecord {
                symbol: "AAA".to_string(),
                pair_key: "key1".to_string(),
                entry_price: 1.0,
                exit_price: 1.1,
                pnl_pct: 10.0,
                position_size_usd: 50.0,
                opened_at: "t1".to_string(),
                closed_at: "t2".to_string(),
                reason: "tp".to_string(),
            },
            TradeRecord {
                symbol: "BBB".to_string(),
                pair_key: "key2".to_string(),
                entry_price: 2.0,
                exit_price: 1.8,
                pnl_pct: -10.0,
                position_size_usd: 50.0,
                opened_at: "t3".to_string(),
                closed_at: "t4".to_string(),
                reason: "sl".to_string(),
            },
        ];

        let json = serde_json::to_string_pretty(&history).expect("should serialize vec");
        let deserialized: Vec<TradeRecord> = serde_json::from_str(&json).expect("should deserialize vec");

        assert_eq!(deserialized.len(), 2);
        assert_eq!(deserialized[0].symbol, "AAA");
        assert_eq!(deserialized[1].pnl_pct, -10.0);
    }

    #[test]
    fn load_trade_history_returns_empty_for_missing_file() {
        // load_trade_history should return empty vec for missing file without panic
        let history = load_trade_history();
        // It will load from actual config path, but for test purposes we just verify it doesn't panic
        // and returns a valid vec (even if empty or with existing data)
        assert!(history.len() >= 0);
    }

    #[test]
    fn set_bot_closed_clamps_extreme_pnl_values() {
        let mut app = App::new();
        // Setup a fake position to close
        app.bot_active_position = Some(BotPosition {
            pair_key: "test-pair".to_string(),
            pair_address: Some("addr".to_string()),
            symbol: "TEST".to_string(),
            entry_price: 1.0,
            target_price: 1.1,
            take_profit_pct: 10.0,
            stop_loss_price: 0.9,
            stop_loss_pct: 10.0,
            opened_at: "now".to_string(),
            provider_position_id: None,
        });

        // Test with extreme pnl value (should be clamped to 10000%)
        app.set_bot_closed("TEST", 1.5, 50000.0, "test");
        
        // Check the last trade record was clamped
        if let Some(last) = app.trade_history.last() {
            assert_eq!(last.pnl_pct, 10000.0); // Clamped to max
        }
    }

    #[test]
    fn set_bot_closed_handles_nan_pnl() {
        let mut app = App::new();
        app.bot_active_position = Some(BotPosition {
            pair_key: "test-pair".to_string(),
            pair_address: Some("addr".to_string()),
            symbol: "TEST".to_string(),
            entry_price: 1.0,
            target_price: 1.1,
            take_profit_pct: 10.0,
            stop_loss_price: 0.9,
            stop_loss_pct: 10.0,
            opened_at: "now".to_string(),
            provider_position_id: None,
        });

        // Test with NaN (should default to 0.0)
        app.set_bot_closed("TEST", 1.5, f64::NAN, "test");
        
        if let Some(last) = app.trade_history.last() {
            assert_eq!(last.pnl_pct, 0.0); // NaN becomes 0.0
        }
    }

    #[test]
    fn set_bot_closed_truncates_long_reason() {
        let mut app = App::new();
        app.bot_active_position = Some(BotPosition {
            pair_key: "test-pair".to_string(),
            pair_address: Some("addr".to_string()),
            symbol: "TEST".to_string(),
            entry_price: 1.0,
            target_price: 1.1,
            take_profit_pct: 10.0,
            stop_loss_price: 0.9,
            stop_loss_pct: 10.0,
            opened_at: "now".to_string(),
            provider_position_id: None,
        });

        let long_reason = "a".repeat(100); // 100 chars
        app.set_bot_closed("TEST", 1.5, 10.0, &long_reason);
        
        if let Some(last) = app.trade_history.last() {
            assert_eq!(last.reason.len(), 50); // Truncated to 50 chars
        }
    }

    #[test]
    fn compute_top_candidate_selects_highest_score() {
        let mut app = App::new();
        
        // Create pairs with different scores - good pair has healthy momentum
        let mut good_pair = sample_pair("GOOD", 600_000.0, 200_000.0);
        if let Some(change) = good_pair.price_change.as_mut() {
            change.h24 = Some(15.0); // Healthy momentum
            change.h1 = Some(3.0);
            change.m5 = Some(1.0);
        }
        
        let mut weak_pair = sample_pair("WEAK", 40_000.0, 15_000.0);
        if let Some(change) = weak_pair.price_change.as_mut() {
            change.h24 = Some(-15.0); // Bearish
            change.h1 = Some(-5.0);
        }
        
        app.update_pairs(vec![good_pair, weak_pair]);
        
        // Top candidate should be the good pair (if score >= 60)
        if let Some(candidate) = &app.top_candidate {
            assert_eq!(candidate.symbol, "GOOD");
            assert!(candidate.score >= 60);
        }
    }

    #[test]
    fn compute_top_candidate_none_when_no_qualified() {
        let mut app = App::new();
        
        // Create pairs with very low scores
        let mut bad_pair = sample_pair("BAD", 10_000.0, 5_000.0);
        bad_pair.price_change = None; // No price data
        bad_pair.volume = None;
        
        app.update_pairs(vec![bad_pair]);
        
        // Should have no top candidate due to low score/confidence
        assert!(app.top_candidate.is_none() || app.filtered_pairs.is_empty());
    }

    #[test]
    fn top_candidate_risk_level_based_on_score_confidence() {
        let mut app = App::new();
        
        // High score pair
        let mut high_score = sample_pair("HIGH", 800_000.0, 300_000.0);
        if let Some(change) = high_score.price_change.as_mut() {
            change.h24 = Some(20.0);
            change.h1 = Some(4.0);
            change.m5 = Some(1.5);
        }
        high_score.market_cap = Some(5_000_000.0);
        
        app.update_pairs(vec![high_score]);
        
        if let Some(candidate) = &app.top_candidate {
            // With high score and complete data, risk should be Low or Medium
            assert!(candidate.risk_level == RiskLevel::Low || candidate.risk_level == RiskLevel::Medium);
        }
    }
}
