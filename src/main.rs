mod api;
mod app;
mod config;
mod logger;
mod rsi;
mod ui;
mod yield_mod;

use app::{App, BotCloseInput, BotOpenInput, BotPosition};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    DefaultTerminal, Frame,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{self, error::TryRecvError, UnboundedReceiver, UnboundedSender};
use tracing::{error, info, warn};

type FetchMessage = Result<api::FetchResult, String>;
type BotMessage = Result<BotTaskResult, String>;
const METEORA_APP_URL: &str = "https://app.meteora.ag/";

enum BotTaskResult {
    Opened(BotPosition),
    Closed {
        symbol: String,
        current_price: f64,
        pnl_pct: f64,
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BotMode {
    Auto,
    Router,
    Direct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BotAuthMode {
    Bearer,
    XApiKey,
    Both,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    config::initialize_active_config().map_err(std::io::Error::other)?;
    let _log_guard = logger::init().map_err(std::io::Error::other)?;
    info!(
        config_path = %config::config_file_path().display(),
        data_dir = %config::app_data_dir().display(),
        "scanner starting"
    );

    let mut terminal = ratatui::init();
    terminal.clear()?;
    show_intro_screen(&mut terminal)?;

    let result = run_app(&mut terminal).await;

    ratatui::restore();
    info!("scanner shutdown");
    result
}

fn show_intro_screen(terminal: &mut DefaultTerminal) -> Result<(), Box<dyn std::error::Error>> {
    let duration = Duration::from_millis(config::active_config().app.intro_duration_ms);
    let started_at = Instant::now();

    loop {
        terminal.draw(|frame| render_intro(frame, started_at.elapsed()))?;

        if started_at.elapsed() >= duration {
            break;
        }

        if event::poll(Duration::from_millis(90))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    break;
                }
            }
        }
    }

    terminal.clear()?;
    Ok(())
}

fn render_intro(frame: &mut Frame, elapsed: Duration) {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Length(8),
            Constraint::Percentage(35),
        ])
        .split(area);

    let dots_count = (elapsed.as_millis() / 300 % 4) as usize;
    let dots = ".".repeat(dots_count);

    let intro_lines = vec![
        Line::styled(
            "Welcome to HALIIM Terminal",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::styled(
            "Catch the Volatility. Own the Flow.",
            Style::default().fg(Color::Yellow),
        ),
        Line::from(""),
        Line::styled(
            format!("Loading market intelligence{dots}"),
            Style::default().fg(Color::Gray),
        ),
        Line::styled(
            "Press any key to skip",
            Style::default().fg(Color::DarkGray),
        ),
    ];

    let intro = Paragraph::new(intro_lines)
        .block(
            Block::default()
                .title(" HALIIM LP SCANNER ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        )
        .alignment(Alignment::Center);

    frame.render_widget(intro, layout[1]);
}

async fn run_app(terminal: &mut DefaultTerminal) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::active_config();
    let mut app = App::new();
    let client = api::build_http_client().map_err(std::io::Error::other)?;
    let (fetch_tx, mut fetch_rx) = mpsc::unbounded_channel::<FetchMessage>();
    let (bot_tx, mut bot_rx) = mpsc::unbounded_channel::<BotMessage>();
    let (yield_tx, mut yield_rx) =
        mpsc::unbounded_channel::<HashMap<String, Vec<yield_mod::YieldInfo>>>();
    let mut fetch_in_flight = false;
    let mut bot_in_flight = false;
    let mut yield_in_flight = false;
    let export_dir = config::resolve_path(&cfg.export.directory);

    // Initial data fetch in background to keep UI responsive.
    request_fetch(&client, &fetch_tx, &mut fetch_in_flight, &mut app);
    request_yield_fetch(&client, &yield_tx, &mut yield_in_flight);

    let tick_rate = Duration::from_millis(cfg.app.tick_rate_ms.max(1));
    let refresh_rate = Duration::from_secs(cfg.app.refresh_rate_secs.max(1));
    let bot_check_rate = Duration::from_secs(cfg.app.bot_check_rate_secs.max(1));
    // Yield data from DefiLlama refreshes every 5 minutes per Ira's research
    let yield_refresh_rate = Duration::from_secs(300);
    let mut last_refresh = Instant::now();
    let mut last_bot_check = Instant::now();
    let mut last_yield_refresh = Instant::now();

    loop {
        drain_fetch_results(&mut fetch_rx, &mut app, &mut fetch_in_flight);
        drain_bot_results(&mut bot_rx, &mut app, &mut bot_in_flight);
        drain_yield_results(&mut yield_rx, &mut app, &mut yield_in_flight);
        terminal.draw(|frame| ui::render(frame, &app))?;

        // Auto-refresh
        if last_refresh.elapsed() >= refresh_rate && !app.show_help && !fetch_in_flight {
            request_fetch(&client, &fetch_tx, &mut fetch_in_flight, &mut app);
            last_refresh = Instant::now();
        }

        // Yield refresh (every 3 minutes for volatile APY)
        if last_refresh.elapsed() >= Duration::from_secs(180) && !yield_in_flight {
            request_yield_fetch(&client, &yield_tx, &mut yield_in_flight);
        }

        if last_bot_check.elapsed() >= bot_check_rate && !bot_in_flight {
            if let Some(payload) = app.auto_take_profit_input() {
                request_bot_close(&client, &bot_tx, &mut bot_in_flight, payload);
            }
            last_bot_check = Instant::now();
        }

        if last_yield_refresh.elapsed() >= yield_refresh_rate && !yield_in_flight {
            request_yield_fetch(&client, &yield_tx, &mut yield_in_flight);
            last_yield_refresh = Instant::now();
        }

        // Handle input with timeout
        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                if app.show_help {
                    match key.code {
                        KeyCode::Char('h') | KeyCode::Esc | KeyCode::Char('q') => {
                            app.toggle_help();
                        }
                        _ => {}
                    }
                    continue;
                }

                if app.search_mode {
                    handle_search_key(&mut app, key);
                    continue;
                }

                if app.open_link_mode {
                    handle_open_link_key(&mut app, key);
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Char('h') => app.toggle_help(),
                    KeyCode::Up | KeyCode::Char('k') => app.move_selection_up(),
                    KeyCode::Down | KeyCode::Char('j') => app.move_selection_down(),
                    KeyCode::Right => {
                        app.sort_column = app.sort_column.next();
                        app.apply_filters();
                    }
                    KeyCode::Left => {
                        app.sort_column = app.sort_column.prev();
                        app.apply_filters();
                    }
                    KeyCode::Char('r') => app.toggle_sort_direction(),
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        app.min_volume_filter += 1000.0;
                        app.apply_filters();
                    }
                    KeyCode::Char('-') | KeyCode::Char('_') => {
                        app.min_volume_filter = (app.min_volume_filter - 1000.0).max(0.0);
                        app.apply_filters();
                    }
                    KeyCode::Char('[') => {
                        app.min_liquidity_filter = (app.min_liquidity_filter - 500.0).max(0.0);
                        app.apply_filters();
                    }
                    KeyCode::Char(']') => {
                        app.min_liquidity_filter += 500.0;
                        app.apply_filters();
                    }
                    KeyCode::Char('{') => {
                        app.min_market_cap_filter =
                            (app.min_market_cap_filter - 10_000.0).max(0.0);
                        app.apply_filters();
                    }
                    KeyCode::Char('}') => {
                        app.min_market_cap_filter += 10_000.0;
                        app.apply_filters();
                    }
                    KeyCode::Enter => {
                        open_selected_pair_in_meteora(&mut app);
                    }
                    KeyCode::Char('o') => {
                        app.start_open_link_mode();
                    }
                    KeyCode::Char('f') | KeyCode::Char('F') => {
                        app.cycle_dex_filter();
                    }
                    KeyCode::Char('w') => {
                        app.toggle_selected_watchlist();
                    }
                    KeyCode::Char('W') => {
                        app.toggle_watchlist_only();
                    }
                    KeyCode::Char('b') => app.toggle_bot_enabled(),
                    KeyCode::Char('p') => {
                        if bot_in_flight {
                            app.set_warning(String::from("Bot action still running"));
                            continue;
                        }
                        if let Some(payload) = app.selected_pair_for_bot_open() {
                            request_bot_open(&client, &bot_tx, &mut bot_in_flight, payload);
                        } else {
                            app.set_warning(String::from(
                                "Cannot open bot position (already open or invalid selected pair)",
                            ));
                        }
                    }
                    KeyCode::Char('t') => {
                        if bot_in_flight {
                            app.set_warning(String::from("Bot action still running"));
                            continue;
                        }
                        if let Some(payload) =
                            app.close_input_for_active_position("manual_take_profit")
                        {
                            request_bot_close(&client, &bot_tx, &mut bot_in_flight, payload);
                        } else {
                            app.set_warning(String::from("No active bot position to close"));
                        }
                    }
                    KeyCode::Char('s') => {
                        app.start_search();
                    }
                    KeyCode::Char('g') | KeyCode::Char('G') => {
                        app.toggle_degen_mode();
                    }
                    KeyCode::Char('R') => {
                        // Force refresh
                        if !fetch_in_flight {
                            request_fetch(&client, &fetch_tx, &mut fetch_in_flight, &mut app);
                            last_refresh = Instant::now();
                        } else {
                            app.set_warning(String::from("Refresh already in progress"));
                        }
                    }
                    KeyCode::Char('x') | KeyCode::Char('X') => {
                        match app.export_filtered_to_csv(&export_dir) {
                            Ok(path) => {
                                app.set_warning(format!("Exported CSV: {}", path.display()));
                                info!(path = %path.display(), "csv export completed");
                            }
                            Err(err) => {
                                app.set_warning(format!("CSV export failed: {err}"));
                                warn!(error = %err, "csv export failed");
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn request_fetch(
    client: &reqwest::Client,
    fetch_tx: &UnboundedSender<FetchMessage>,
    fetch_in_flight: &mut bool,
    app: &mut App,
) {
    if *fetch_in_flight {
        return;
    }

    *fetch_in_flight = true;
    app.is_loading = true;
    app.set_warning(String::from(
        "Fetching data market (DexScreener/Jupiter fallback)...",
    ));

    let tx = fetch_tx.clone();
    let client = client.clone();
    tokio::spawn(async move {
        let _ = tx.send(api::fetch_volatile_pairs(&client).await);
    });
}

fn drain_fetch_results(
    fetch_rx: &mut UnboundedReceiver<FetchMessage>,
    app: &mut App,
    fetch_in_flight: &mut bool,
) {
    loop {
        match fetch_rx.try_recv() {
            Ok(result) => {
                *fetch_in_flight = false;
                match result {
                    Ok(data) => {
                        app.update_pairs(data.pairs);
                        let alerts = app.collect_price_alerts();
                        if !alerts.is_empty() {
                            for alert in &alerts {
                                warn!(alert = %alert, "price alert triggered");
                            }
                            if config::active_config().alerts.terminal_bell {
                                for _ in 0..alerts.len().min(3) {
                                    let _ = emit_terminal_bell();
                                }
                            }
                            app.set_warning(alerts.join(" | "));
                        }
                        if !data.warnings.is_empty() {
                            let warnings = data.warnings.join(" | ");
                            warn!(warnings = %warnings, "fetch warnings");
                            app.set_warning(format!("Loaded with warnings: {warnings}"));
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "fetch failed");
                        app.set_error(e);
                    }
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                *fetch_in_flight = false;
                error!("background fetch channel disconnected");
                app.set_error(String::from("Background fetch channel disconnected"));
                break;
            }
        }
    }
}

fn emit_terminal_bell() -> std::io::Result<()> {
    print!("\x07");
    std::io::stdout().flush()
}

fn request_bot_open(
    client: &reqwest::Client,
    bot_tx: &UnboundedSender<BotMessage>,
    bot_in_flight: &mut bool,
    payload: BotOpenInput,
) {
    if *bot_in_flight {
        return;
    }
    *bot_in_flight = true;

    let tx = bot_tx.clone();
    let client = client.clone();
    tokio::spawn(async move {
        let result = execute_bot_open(&client, payload).await;
        let _ = tx.send(result);
    });
}

fn request_bot_close(
    client: &reqwest::Client,
    bot_tx: &UnboundedSender<BotMessage>,
    bot_in_flight: &mut bool,
    payload: BotCloseInput,
) {
    if *bot_in_flight {
        return;
    }
    *bot_in_flight = true;

    let tx = bot_tx.clone();
    let client = client.clone();
    tokio::spawn(async move {
        let result = execute_bot_close(&client, payload).await;
        let _ = tx.send(result);
    });
}

fn drain_bot_results(
    bot_rx: &mut UnboundedReceiver<BotMessage>,
    app: &mut App,
    bot_in_flight: &mut bool,
) {
    loop {
        match bot_rx.try_recv() {
            Ok(result) => {
                *bot_in_flight = false;
                match result {
                    Ok(BotTaskResult::Opened(position)) => {
                        app.set_bot_opened(position);
                    }
                    Ok(BotTaskResult::Closed {
                        symbol,
                        current_price,
                        pnl_pct,
                        reason,
                    }) => {
                        app.set_bot_closed(&symbol, current_price, pnl_pct, &reason);
                    }
                    Err(err) => {
                        app.set_bot_error("REQUEST", &err);
                    }
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                *bot_in_flight = false;
                app.set_bot_error("CHANNEL", "Bot channel disconnected");
                break;
            }
        }
    }
}

fn request_yield_fetch(
    client: &reqwest::Client,
    yield_tx: &UnboundedSender<HashMap<String, Vec<yield_mod::YieldInfo>>>,
    yield_in_flight: &mut bool,
) {
    if *yield_in_flight {
        return;
    }

    *yield_in_flight = true;
    let tx = yield_tx.clone();
    let client = client.clone();
    tokio::spawn(async move {
        let result = yield_mod::fetch_yields(&client).await;
        match result {
            Ok(yields) => {
                let _ = tx.send(yields);
            }
            Err(err) => {
                warn!("Failed to fetch yield data: {}", err);
            }
        }
    });
}

fn drain_yield_results(
    yield_rx: &mut UnboundedReceiver<HashMap<String, Vec<yield_mod::YieldInfo>>>,
    app: &mut App,
    yield_in_flight: &mut bool,
) {
    loop {
        match yield_rx.try_recv() {
            Ok(yields) => {
                *yield_in_flight = false;
                app.update_yields(yields);
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                *yield_in_flight = false;
                break;
            }
        }
    }
}

async fn execute_bot_open(
    client: &reqwest::Client,
    payload: BotOpenInput,
) -> Result<BotTaskResult, String> {
    let api_key = bot_api_key();
    let body = json!({
        "pairKey": payload.pair_key,
        "pairAddress": payload.pair_address,
        "symbol": payload.symbol,
        "baseMint": payload.base_mint,
        "quoteMint": payload.quote_mint,
        "entryPrice": payload.entry_price,
        "sizeUsd": payload.position_size_usd,
        "takeProfitPct": payload.take_profit_pct,
        "stopLossPct": payload.stop_loss_pct,
        "source": "haliim-terminal"
    });

    let action_payload = body.clone();
    let endpoint = bot_open_endpoint();
    let fallback_endpoints = bot_open_endpoint_fallbacks();
    let response = post_bot_request_with_not_found_fallback(
        client,
        &endpoint,
        &fallback_endpoints,
        &body,
        &api_key,
        "open",
        &action_payload,
    )
    .await?;
    validate_bot_response(&response)?;

    let target_price = payload.entry_price * (1.0 + payload.take_profit_pct / 100.0);
    let stop_loss_price = payload.entry_price * (1.0 - payload.stop_loss_pct / 100.0);
    let position_id = extract_string(&response, &["positionId", "id"]);
    let position = BotPosition {
        pair_key: payload.pair_key,
        pair_address: payload.pair_address,
        symbol: payload.symbol,
        entry_price: payload.entry_price,
        target_price,
        take_profit_pct: payload.take_profit_pct,
        stop_loss_price,
        stop_loss_pct: payload.stop_loss_pct,
        opened_at: chrono::Local::now().format("%H:%M:%S").to_string(),
        provider_position_id: position_id,
    };

    Ok(BotTaskResult::Opened(position))
}

async fn execute_bot_close(
    client: &reqwest::Client,
    payload: BotCloseInput,
) -> Result<BotTaskResult, String> {
    let api_key = bot_api_key();
    let body = json!({
        "pairKey": payload.pair_key,
        "pairAddress": payload.pair_address,
        "symbol": payload.symbol,
        "entryPrice": payload.entry_price,
        "exitPrice": payload.current_price,
        "positionId": payload.provider_position_id,
        "reason": payload.reason,
        "source": "haliim-terminal"
    });

    let action_payload = body.clone();
    let endpoint = bot_close_endpoint();
    let fallback_endpoints = bot_close_endpoint_fallbacks();
    let response = post_bot_request_with_not_found_fallback(
        client,
        &endpoint,
        &fallback_endpoints,
        &body,
        &api_key,
        "close",
        &action_payload,
    )
    .await?;
    validate_bot_response(&response)?;

    let pnl_pct = ((payload.current_price - payload.entry_price) / payload.entry_price) * 100.0;
    Ok(BotTaskResult::Closed {
        symbol: payload.symbol,
        current_price: payload.current_price,
        pnl_pct,
        reason: payload.reason,
    })
}

async fn post_bot_request(
    client: &reqwest::Client,
    base_url: &str,
    endpoint: &str,
    body: &Value,
    api_key: &str,
) -> Result<Value, (Option<reqwest::StatusCode>, String)> {
    let url = format!("{}/{}", base_url.trim_end_matches('/'), endpoint);
    let mut request = client.post(&url).json(body);

    match bot_auth_mode() {
        BotAuthMode::Bearer => {
            request = request.header("Authorization", format!("Bearer {api_key}"));
        }
        BotAuthMode::XApiKey => {
            request = request.header("X-API-Key", api_key);
        }
        BotAuthMode::Both => {
            request = request.header("Authorization", format!("Bearer {api_key}"));
            request = request.header("X-API-Key", api_key);
        }
    }

    let response = request
        .send()
        .await
        .map_err(|err| (None, format!("{url} request error: {err}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err((Some(status), format!("{url} failed with {status}: {text}")));
    }

    let status = response.status();
    response
        .json::<Value>()
        .await
        .map_err(|err| (Some(status), format!("{url} invalid JSON response: {err}")))
}

async fn post_bot_request_with_not_found_fallback(
    client: &reqwest::Client,
    primary_endpoint: &str,
    fallback_endpoints: &[String],
    body: &Value,
    api_key: &str,
    action: &str,
    action_payload: &Value,
) -> Result<Value, String> {
    let prefer_router = should_prefer_router();
    let mut router_first_error: Option<String> = None;
    if prefer_router {
        match post_via_9router_chat(client, api_key, action, action_payload).await {
            Ok(value) => return Ok(value),
            Err(err) => {
                router_first_error = Some(err);
            }
        }
    }

    let mut endpoint_candidates = vec![primary_endpoint.to_string()];
    for candidate in fallback_endpoints {
        if candidate != primary_endpoint {
            endpoint_candidates.push(candidate.to_string());
        }
    }

    let base_candidates = bot_api_base_url_candidates();
    let mut errors = Vec::new();

    for base_url in &base_candidates {
        for endpoint in &endpoint_candidates {
            match post_bot_request(client, base_url, endpoint, body, api_key).await {
                Ok(value) => return Ok(value),
                Err((Some(status), err)) if status == reqwest::StatusCode::NOT_FOUND => {
                    errors.push(err);
                    continue;
                }
                Err((_status, err)) => return Err(err),
            }
        }
    }

    let bases = base_candidates.join(", ");
    let endpoints = endpoint_candidates.join(", ");
    if errors.is_empty() {
        if let Some(router_err) = router_first_error {
            return Err(format!(
                "9router route gagal: {router_err} | Bot API 404 pada semua kombinasi base/endpoint. bases=[{bases}] endpoints=[{endpoints}]"
            ));
        }
        return Err(format!(
            "Bot API 404 pada semua kombinasi base/endpoint. bases=[{bases}] endpoints=[{endpoints}]"
        ));
    }

    if let Some(router_err) = router_first_error {
        return Err(format!(
            "9router route gagal: {router_err} | {} | Cek file config.toml bagian [bot]. tried bases=[{bases}] endpoints=[{endpoints}]",
            errors.join(" | ")
        ));
    }

    let router_result = post_via_9router_chat(client, api_key, action, action_payload).await;
    match router_result {
        Ok(value) => Ok(value),
        Err(router_err) => Err(format!(
            "{} | 9router fallback gagal: {router_err} | Cek file config.toml bagian [bot]. tried bases=[{bases}] endpoints=[{endpoints}]",
            errors.join(" | ")
        )),
    }
}

fn bot_api_key() -> String {
    config::active_config().bot.api_key.clone()
}

fn bot_api_base_url() -> String {
    config::active_config().bot.api_base_url.clone()
}

fn bot_api_base_url_candidates() -> Vec<String> {
    let configured = bot_api_base_url().trim_end_matches('/').to_string();
    let mut candidates = vec![configured.clone()];

    if let Some(stripped) = configured.strip_suffix("/v1") {
        let stripped = stripped.to_string();
        if !stripped.is_empty() {
            candidates.push(stripped.clone());
            candidates.push(format!("{stripped}/api/v1"));
        }
    } else {
        candidates.push(format!("{configured}/v1"));
        candidates.push(format!("{configured}/api/v1"));
    }

    candidates.retain(|x| !x.is_empty());
    candidates.dedup();
    candidates
}

fn bot_open_endpoint() -> String {
    config::active_config().bot.open_endpoint.clone()
}

fn bot_close_endpoint() -> String {
    config::active_config().bot.close_endpoint.clone()
}

fn bot_open_endpoint_fallbacks() -> Vec<String> {
    config::active_config().bot.open_endpoint_fallbacks.clone()
}

fn bot_close_endpoint_fallbacks() -> Vec<String> {
    config::active_config().bot.close_endpoint_fallbacks.clone()
}

fn bot_router_chat_endpoint() -> String {
    config::active_config().bot.router_chat_endpoint.clone()
}

fn bot_router_model() -> String {
    config::active_config().bot.router_model.clone()
}

fn bot_router_models() -> Vec<String> {
    let configured = config::active_config().bot.router_models.clone();

    let mut models = Vec::new();
    if !configured.is_empty() {
        models.extend(configured);
    } else {
        models.push(bot_router_model());
    }

    models.push("openrouter/auto".to_string());
    models.push("gpt-4o-mini".to_string());
    models.retain(|m| !m.is_empty());
    models.dedup();
    models
}

fn bot_execution_mode() -> BotMode {
    parse_bot_mode(config::active_config().bot.execution_mode.as_str())
}

fn bot_auth_mode() -> BotAuthMode {
    parse_bot_auth_mode(config::active_config().bot.auth_mode.as_str())
}

fn parse_bot_mode(mode: &str) -> BotMode {
    match mode.to_ascii_lowercase().as_str() {
        "router" => BotMode::Router,
        "direct" => BotMode::Direct,
        _ => BotMode::Auto,
    }
}

fn parse_bot_auth_mode(mode: &str) -> BotAuthMode {
    match mode.to_ascii_lowercase().as_str() {
        "bearer" => BotAuthMode::Bearer,
        "x-api-key" | "xapikey" | "x_api_key" => BotAuthMode::XApiKey,
        _ => BotAuthMode::Both,
    }
}

fn should_prefer_router() -> bool {
    match bot_execution_mode() {
        BotMode::Router => true,
        BotMode::Direct => false,
        BotMode::Auto => {
            let base = bot_api_base_url().to_ascii_lowercase();
            base.contains("/v1")
        }
    }
}

fn extract_first_choice_content(response: &Value) -> Option<String> {
    response
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|choices| choices.first())
        .and_then(|first| first.get("message"))
        .and_then(|msg| msg.get("content"))
        .and_then(|content| content.as_str())
        .map(|s| s.to_string())
}

fn extract_json_object_from_text(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(&text[start..=end])
}

async fn post_via_9router_chat(
    client: &reqwest::Client,
    api_key: &str,
    action: &str,
    action_payload: &Value,
) -> Result<Value, String> {
    let prompt = format!(
        "Execute trading action '{action}' with payload JSON below. \
Return ONLY strict JSON object with keys: success(boolean), positionId(string optional), error(string optional), message(string optional).\n\
Payload: {action_payload}"
    );
    let base_candidates = bot_api_base_url_candidates();
    let router_endpoint = bot_router_chat_endpoint();
    let model_candidates = bot_router_models();
    let mut errors = Vec::new();

    for base in &base_candidates {
        for model in &model_candidates {
            let body = json!({
                "model": model,
                "stream": false,
                "messages": [
                    {
                        "role": "system",
                        "content": "You are a trading execution bot. Always return strict JSON only."
                    },
                    {
                        "role": "user",
                        "content": prompt
                    }
                ]
            });

            let response =
                match post_bot_request(client, base, &router_endpoint, &body, api_key).await {
                    Ok(v) => v,
                    Err((_status, err)) => {
                        errors.push(err);
                        continue;
                    }
                };

            let Some(content) = extract_first_choice_content(&response) else {
                errors.push(format!(
                    "9router chat response tidak punya message content: {response}"
                ));
                continue;
            };

            let Some(json_text) = extract_json_object_from_text(&content) else {
                errors.push(format!(
                    "9router chat response bukan JSON object valid: {content}"
                ));
                continue;
            };

            let parsed = match serde_json::from_str::<Value>(json_text) {
                Ok(v) => v,
                Err(err) => {
                    errors.push(format!("gagal parse JSON dari 9router content: {err}"));
                    continue;
                }
            };

            return Ok(parsed);
        }
    }

    let bases = base_candidates.join(", ");
    let models = model_candidates.join(", ");
    Err(format!(
        "{} | tried router bases=[{bases}] models=[{models}]",
        errors.join(" | ")
    ))
}

fn validate_bot_response(response: &Value) -> Result<(), String> {
    if let Some(err_msg) = response
        .get("error")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .or_else(|| {
            response
                .get("data")
                .and_then(|data| data.get("error"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
    {
        return Err(format!("Bot API error: {err_msg}"));
    }

    if let Some(success) = response
        .get("success")
        .and_then(|value| value.as_bool())
        .or_else(|| {
            response
                .get("data")
                .and_then(|data| data.get("success"))
                .and_then(|value| value.as_bool())
        })
    {
        if !success {
            return Err(format!("Bot API returned success=false: {response}"));
        }
    }

    Ok(())
}

fn extract_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(v) = value.get(key).and_then(|raw| raw.as_str()) {
            return Some(v.to_string());
        }
        if let Some(v) = value
            .get("data")
            .and_then(|data| data.get(key))
            .and_then(|raw| raw.as_str())
        {
            return Some(v.to_string());
        }
    }
    None
}

fn handle_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => app.stop_search_keep_query(),
        KeyCode::Esc => app.stop_search_clear_query(),
        KeyCode::Backspace => {
            app.search_query.pop();
            app.apply_filters();
        }
        KeyCode::Char(c) => {
            app.search_query.push(c);
            app.apply_filters();
        }
        _ => {}
    }
}

fn handle_open_link_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.stop_open_link_mode();
            app.set_warning(String::from("Open mode cancelled"));
        }
        KeyCode::Enter | KeyCode::Char('m') | KeyCode::Char('M') => {
            app.stop_open_link_mode();
            open_selected_pair_in_meteora(app);
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            app.stop_open_link_mode();
            open_selected_pair_in_dexscreener(app);
        }
        _ => {}
    }
}

fn open_selected_pair_in_meteora(app: &mut App) {
    if let Some(pair) = app.selected_pair() {
        let url = build_meteora_url(pair);
        if let Err(err) = open::that(&url) {
            app.set_warning(format!("Failed to open Meteora URL: {err}"));
        } else {
            app.set_warning(String::from("Opened pair in Meteora"));
        }
    }
}

fn open_selected_pair_in_dexscreener(app: &mut App) {
    if let Some(pair) = app.selected_pair() {
        if let Some(url) = pair.url.as_ref() {
            if let Err(err) = open::that(url) {
                app.set_warning(format!("Failed to open DexScreener URL: {err}"));
            } else {
                app.set_warning(String::from("Opened pair in DexScreener"));
            }
        } else {
            app.set_warning(String::from(
                "DexScreener URL is not available for this pair",
            ));
        }
    }
}

fn build_meteora_url(pair: &api::PairData) -> String {
    let native_sol_mint = &config::active_config().api.jupiter_input_mint;
    let base_mint = pair.base_token.address.as_deref();
    let quote_mint = pair.quote_token.address.as_deref();

    let output_mint = match base_mint {
        Some(mint) if !mint.eq_ignore_ascii_case(native_sol_mint) => Some(mint),
        _ => quote_mint.filter(|mint| !mint.eq_ignore_ascii_case(native_sol_mint)),
    };

    if let Some(output_mint) = output_mint {
        return format!("{METEORA_APP_URL}?inputMint={native_sol_mint}&outputMint={output_mint}");
    }

    METEORA_APP_URL.to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        bot_api_base_url_candidates, bot_router_models, build_meteora_url, parse_bot_auth_mode,
        parse_bot_mode, should_prefer_router, validate_bot_response, BotAuthMode, BotMode,
    };
    use crate::{
        api::{LiquidityData, PairData, PriceChange, TokenInfo, VolumeData},
        config,
    };
    use serde_json::json;
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn init_test_config() {
        INIT.call_once(|| {
            let _ = config::initialize_active_config();
        });
    }

    fn sample_pair(base_mint: Option<&str>, quote_mint: Option<&str>) -> PairData {
        PairData {
            chain_id: "solana".to_string(),
            dex_id: "raydium".to_string(),
            base_token: TokenInfo {
                name: Some("AAA".to_string()),
                symbol: Some("AAA".to_string()),
                address: base_mint.map(str::to_string),
            },
            quote_token: TokenInfo {
                name: Some("USDC".to_string()),
                symbol: Some("USDC".to_string()),
                address: quote_mint.map(str::to_string),
            },
            price_usd: Some("1".to_string()),
            price_change: Some(PriceChange {
                m5: Some(0.0),
                h1: Some(0.0),
                h6: Some(0.0),
                h24: Some(0.0),
            }),
            txns: None,
            volume: Some(VolumeData {
                h24: Some(1.0),
                h6: Some(1.0),
                h1: Some(1.0),
                m5: Some(1.0),
            }),
            liquidity: Some(LiquidityData { usd: Some(1.0) }),
            market_cap: Some(1_000_000.0),
            fdv: Some(1.0),
            pair_created_at: Some(0),
            pair_address: Some("pair".to_string()),
            url: Some("https://dexscreener.com".to_string()),
        }
    }

    #[test]
    fn build_meteora_url_prefers_base_mint() {
        init_test_config();
        let pair = sample_pair(
            Some("TokenMint12345678901234567890123456789012345"),
            Some("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"),
        );
        let url = build_meteora_url(&pair);
        assert!(url.contains("outputMint=TokenMint12345678901234567890123456789012345"));
    }

    #[test]
    fn build_meteora_url_falls_back_to_home_when_missing_mints() {
        init_test_config();
        let pair = sample_pair(None, None);
        let url = build_meteora_url(&pair);
        assert_eq!(url, "https://app.meteora.ag/");
    }

    #[test]
    fn validate_bot_response_rejects_error_payload() {
        let payload = json!({ "error": "insufficient_balance" });
        assert!(validate_bot_response(&payload).is_err());
    }

    #[test]
    fn validate_bot_response_rejects_success_false() {
        let payload = json!({ "success": false, "message": "failed" });
        assert!(validate_bot_response(&payload).is_err());
    }

    #[test]
    fn validate_bot_response_accepts_success_true() {
        let payload = json!({ "success": true, "positionId": "abc123" });
        assert!(validate_bot_response(&payload).is_ok());
    }

    #[test]
    fn bot_api_base_url_candidates_generate_variants() {
        init_test_config();
        let candidates = bot_api_base_url_candidates();
        assert!(candidates.contains(&"http://localhost:20128/v1".to_string()));
        assert!(candidates.contains(&"http://localhost:20128".to_string()));
        assert!(candidates.contains(&"http://localhost:20128/api/v1".to_string()));
    }

    #[test]
    fn bot_modes_are_parsed_from_config_values() {
        assert_eq!(parse_bot_mode("router"), BotMode::Router);
        assert_eq!(parse_bot_mode("direct"), BotMode::Direct);
        assert_eq!(parse_bot_mode("auto"), BotMode::Auto);
        assert_eq!(parse_bot_mode("unknown"), BotMode::Auto);
        init_test_config();
        assert!(should_prefer_router());
    }

    #[test]
    fn bot_auth_modes_are_parsed_from_config_values() {
        assert_eq!(parse_bot_auth_mode("bearer"), BotAuthMode::Bearer);
        assert_eq!(parse_bot_auth_mode("x-api-key"), BotAuthMode::XApiKey);
        assert_eq!(parse_bot_auth_mode("both"), BotAuthMode::Both);
        assert_eq!(parse_bot_auth_mode("unknown"), BotAuthMode::Both);
    }

    #[test]
    fn bot_router_models_include_defaults() {
        init_test_config();
        let models = bot_router_models();
        assert!(models.contains(&"kr/claude-opus-4.6".to_string()));
        assert!(models.contains(&"openrouter/auto".to_string()));
    }
}
