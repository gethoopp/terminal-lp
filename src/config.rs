use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static ACTIVE_CONFIG: OnceLock<ScannerConfig> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScannerConfig {
    pub api: ApiConfig,
    pub app: AppConfig,
    pub bot: BotConfig,
    pub alerts: AlertConfig,
    pub export: ExportConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiConfig {
    pub http_timeout_secs: u64,
    pub dexscreener_base_url: String,
    pub queries: Vec<String>,
    pub query_delay_ms: u64,
    pub max_retries: usize,
    pub top_pairs_by_volume: usize,
    pub top_pairs_final: usize,
    pub jupiter_enabled: bool,
    pub jupiter_quote_urls: Vec<String>,
    pub jupiter_api_key: String,
    pub jupiter_input_mint: String,
    pub jupiter_output_mint: String,
    pub jupiter_input_decimals: u32,
    pub jupiter_output_decimals: u32,
    pub jupiter_amount: u64,
    pub jupiter_slippage_bps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub tick_rate_ms: u64,
    pub refresh_rate_secs: u64,
    pub bot_check_rate_secs: u64,
    pub intro_duration_ms: u64,
    pub initial_min_volume_filter: f64,
    pub initial_min_liquidity_filter: f64,
    pub initial_min_market_cap_filter: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BotConfig {
    pub api_key: String,
    pub api_base_url: String,
    pub open_endpoint: String,
    pub close_endpoint: String,
    pub open_endpoint_fallbacks: Vec<String>,
    pub close_endpoint_fallbacks: Vec<String>,
    pub router_chat_endpoint: String,
    pub router_model: String,
    pub router_models: Vec<String>,
    pub execution_mode: String,
    pub auth_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertConfig {
    pub enabled: bool,
    pub change_24h_threshold_pct: f64,
    pub cooldown_secs: u64,
    pub terminal_bell: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportConfig {
    pub directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub directory: String,
    pub file_name: String,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        let data_dir = default_data_dir();
        let export_dir = data_dir.join("exports");
        Self {
            api: ApiConfig::default(),
            app: AppConfig::default(),
            bot: BotConfig::default(),
            alerts: AlertConfig::default(),
            export: ExportConfig {
                directory: export_dir.to_string_lossy().to_string(),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                directory: data_dir.to_string_lossy().to_string(),
                file_name: "scanner.log".to_string(),
            },
        }
    }
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            http_timeout_secs: 15,
            dexscreener_base_url: "https://api.dexscreener.com/latest/dex".to_string(),
            queries: vec!["meteora", "SOL", "USDC", "BONK", "WIF", "JUP"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            query_delay_ms: 250,
            max_retries: 1,
            top_pairs_by_volume: 100,
            top_pairs_final: 100,
            jupiter_enabled: true,
            jupiter_quote_urls: vec![
                "https://quote-api.jup.ag/v6/quote".to_string(),
                "https://api.jup.ag/swap/v1/quote".to_string(),
            ],
            jupiter_api_key: String::new(),
            jupiter_input_mint: "So11111111111111111111111111111111111111112".to_string(),
            jupiter_output_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
            jupiter_input_decimals: 9,
            jupiter_output_decimals: 6,
            jupiter_amount: 1_000_000,
            jupiter_slippage_bps: 50,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            tick_rate_ms: 200,
            refresh_rate_secs: 15,
            bot_check_rate_secs: 15,
            intro_duration_ms: 2300,
            initial_min_volume_filter: 1000.0,
            initial_min_liquidity_filter: 500.0,
            initial_min_market_cap_filter: 50_000.0,
        }
    }
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            api_key: "".to_string(),
            api_base_url: "http://localhost:20128/v1".to_string(),
            open_endpoint: "positions/open".to_string(),
            close_endpoint: "positions/close".to_string(),
            open_endpoint_fallbacks: vec!["position/open".to_string(), "trade/open".to_string()],
            close_endpoint_fallbacks: vec!["position/close".to_string(), "trade/close".to_string()],
            router_chat_endpoint: "chat/completions".to_string(),
            router_model: "kr/claude-opus-4.6".to_string(),
            router_models: vec![],
            execution_mode: "auto".to_string(),
            auth_mode: "both".to_string(),
        }
    }
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            change_24h_threshold_pct: 25.0,
            cooldown_secs: 120,
            terminal_bell: true,
        }
    }
}

impl Default for ExportConfig {
    fn default() -> Self {
        let data_dir = default_data_dir();
        Self {
            directory: data_dir.join("exports").to_string_lossy().to_string(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        let data_dir = default_data_dir();
        Self {
            level: "info".to_string(),
            directory: data_dir.to_string_lossy().to_string(),
            file_name: "scanner.log".to_string(),
        }
    }
}

pub fn initialize_active_config() -> Result<&'static ScannerConfig, String> {
    if let Some(cfg) = ACTIVE_CONFIG.get() {
        return Ok(cfg);
    }

    let cfg = load_or_create()?;
    let _ = ACTIVE_CONFIG.set(cfg);
    ACTIVE_CONFIG
        .get()
        .ok_or_else(|| "Config failed to initialize".to_string())
}

pub fn active_config() -> &'static ScannerConfig {
    ACTIVE_CONFIG
        .get()
        .expect("active config must be initialized before use")
}

pub fn active_config_opt() -> Option<&'static ScannerConfig> {
    ACTIVE_CONFIG.get()
}

pub fn config_file_path() -> PathBuf {
    default_config_dir().join("config.toml")
}

pub fn app_data_dir() -> PathBuf {
    default_data_dir()
}

pub fn watchlist_file_path() -> PathBuf {
    app_data_dir().join("watchlist.json")
}

pub fn bot_position_file_path() -> PathBuf {
    app_data_dir().join("bot_position.json")
}

pub fn trade_history_file_path() -> PathBuf {
    app_data_dir().join("trade_history.json")
}

pub fn resolve_path(path_like: &str) -> PathBuf {
    if let Some(rest) = path_like.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    Path::new(path_like).to_path_buf()
}

fn load_or_create() -> Result<ScannerConfig, String> {
    let config_path = config_file_path();
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed creating config dir {}: {err}", parent.display()))?;
    }

    if !config_path.exists() {
        let default_cfg = ScannerConfig::default();
        let toml = toml::to_string_pretty(&default_cfg)
            .map_err(|err| format!("serialize default config failed: {err}"))?;
        fs::write(&config_path, toml).map_err(|err| {
            format!(
                "write default config {} failed: {err}",
                config_path.display()
            )
        })?;
        return Ok(default_cfg);
    }

    let raw = fs::read_to_string(&config_path)
        .map_err(|err| format!("read config {} failed: {err}", config_path.display()))?;
    let cfg: ScannerConfig = toml::from_str(&raw)
        .map_err(|err| format!("parse config {} failed: {err}", config_path.display()))?;
    Ok(cfg)
}

fn default_config_dir() -> PathBuf {
    if let Some(dir) = dirs::config_dir() {
        return dir.join("haliim-scanner");
    }

    if let Some(home) = dirs::home_dir() {
        return home.join(".config").join("haliim-scanner");
    }

    PathBuf::from(".haliim-scanner")
}

fn default_data_dir() -> PathBuf {
    if let Some(dir) = dirs::data_local_dir() {
        return dir.join("haliim-scanner");
    }

    if let Some(home) = dirs::home_dir() {
        return home.join(".local").join("share").join("haliim-scanner");
    }

    PathBuf::from(".haliim-scanner-data")
}
