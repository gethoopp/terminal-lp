mod api;
mod app;
mod ui;

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
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{self, error::TryRecvError, UnboundedReceiver, UnboundedSender};

type FetchMessage = Result<api::FetchResult, String>;
type BotMessage = Result<BotTaskResult, String>;
const METEORA_APP_URL: &str = "https://app.meteora.ag/";
const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
const BOT_API_BASE_URL: &str = "http://localhost:20128/v1";
const BOT_API_KEY: &str = "REPLACE_WITH_LOCAL_BOT_API_KEY";

enum BotTaskResult {
    Opened(BotPosition),
    Closed {
        symbol: String,
        current_price: f64,
        pnl_pct: f64,
        reason: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();
    terminal.clear()?;
    show_intro_screen(&mut terminal)?;

    let result = run_app(&mut terminal).await;

    ratatui::restore();
    result
}

fn show_intro_screen(terminal: &mut DefaultTerminal) -> Result<(), Box<dyn std::error::Error>> {
    let duration = Duration::from_millis(2300);
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
    let mut app = App::new();
    let client = api::build_http_client().map_err(std::io::Error::other)?;
    let (fetch_tx, mut fetch_rx) = mpsc::unbounded_channel::<FetchMessage>();
    let (bot_tx, mut bot_rx) = mpsc::unbounded_channel::<BotMessage>();
    let mut fetch_in_flight = false;
    let mut bot_in_flight = false;

    // Initial data fetch in background to keep UI responsive.
    request_fetch(&client, &fetch_tx, &mut fetch_in_flight, &mut app);

    let tick_rate = Duration::from_millis(200);
    let refresh_rate = Duration::from_secs(15);
    let bot_check_rate = Duration::from_secs(2);
    let mut last_refresh = Instant::now();
    let mut last_bot_check = Instant::now();

    loop {
        drain_fetch_results(&mut fetch_rx, &mut app, &mut fetch_in_flight);
        drain_bot_results(&mut bot_rx, &mut app, &mut bot_in_flight);
        terminal.draw(|frame| ui::render(frame, &app))?;

        // Auto-refresh
        if last_refresh.elapsed() >= refresh_rate && !app.show_help && !fetch_in_flight {
            request_fetch(&client, &fetch_tx, &mut fetch_in_flight, &mut app);
            last_refresh = Instant::now();
        }

        if last_bot_check.elapsed() >= bot_check_rate && !bot_in_flight {
            if let Some(payload) = app.auto_take_profit_input() {
                request_bot_close(&client, &bot_tx, &mut bot_in_flight, payload);
            }
            last_bot_check = Instant::now();
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
                            (app.min_market_cap_filter - 100_000.0).max(0.0);
                        app.apply_filters();
                    }
                    KeyCode::Char('}') => {
                        app.min_market_cap_filter += 100_000.0;
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
                    KeyCode::Char('R') => {
                        // Force refresh
                        if !fetch_in_flight {
                            request_fetch(&client, &fetch_tx, &mut fetch_in_flight, &mut app);
                            last_refresh = Instant::now();
                        } else {
                            app.set_warning(String::from("Refresh already in progress"));
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
    app.set_warning(String::from("Fetching data from DexScreener..."));

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
                        if !data.warnings.is_empty() {
                            let warnings = data.warnings.join(" | ");
                            app.set_warning(format!("Loaded with warnings: {warnings}"));
                        }
                    }
                    Err(e) => {
                        app.set_error(e);
                    }
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                *fetch_in_flight = false;
                app.set_error(String::from("Background fetch channel disconnected"));
                break;
            }
        }
    }
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

async fn execute_bot_open(
    client: &reqwest::Client,
    payload: BotOpenInput,
) -> Result<BotTaskResult, String> {
    let body = json!({
        "pairKey": payload.pair_key,
        "pairAddress": payload.pair_address,
        "symbol": payload.symbol,
        "baseMint": payload.base_mint,
        "quoteMint": payload.quote_mint,
        "entryPrice": payload.entry_price,
        "sizeUsd": payload.position_size_usd,
        "takeProfitPct": payload.take_profit_pct,
        "source": "haliim-terminal"
    });

    let response = post_bot_with_fallback(
        client,
        &["positions/open", "position/open", "trade/open"],
        &body,
    )
    .await?;

    let target_price = payload.entry_price * (1.0 + payload.take_profit_pct / 100.0);
    let position_id = extract_string(&response, &["positionId", "id"]);
    let position = BotPosition {
        pair_key: payload.pair_key,
        pair_address: payload.pair_address,
        symbol: payload.symbol,
        entry_price: payload.entry_price,
        target_price,
        take_profit_pct: payload.take_profit_pct,
        opened_at: chrono::Local::now().format("%H:%M:%S").to_string(),
        provider_position_id: position_id,
    };

    Ok(BotTaskResult::Opened(position))
}

async fn execute_bot_close(
    client: &reqwest::Client,
    payload: BotCloseInput,
) -> Result<BotTaskResult, String> {
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

    post_bot_with_fallback(
        client,
        &["positions/close", "position/close", "trade/close"],
        &body,
    )
    .await?;

    let pnl_pct = ((payload.current_price - payload.entry_price) / payload.entry_price) * 100.0;
    Ok(BotTaskResult::Closed {
        symbol: payload.symbol,
        current_price: payload.current_price,
        pnl_pct,
        reason: payload.reason,
    })
}

async fn post_bot_with_fallback(
    client: &reqwest::Client,
    endpoints: &[&str],
    body: &Value,
) -> Result<Value, String> {
    let mut last_error = String::from("Unknown bot API error");

    for endpoint in endpoints {
        let url = format!("{}/{}", BOT_API_BASE_URL.trim_end_matches('/'), endpoint);
        let response = client
            .post(&url)
            .header("X-API-Key", BOT_API_KEY)
            .header("Authorization", format!("Bearer {BOT_API_KEY}"))
            .json(body)
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                let parsed = resp
                    .json::<Value>()
                    .await
                    .unwrap_or_else(|_| json!({ "ok": true }));
                return Ok(parsed);
            }
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                last_error = format!("{endpoint} failed with {status}: {text}");
            }
            Err(err) => {
                last_error = format!("{endpoint} request error: {err}");
            }
        }
    }

    Err(last_error)
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
    let base_mint = pair.base_token.address.as_deref();
    let quote_mint = pair.quote_token.address.as_deref();

    let output_mint = match base_mint {
        Some(mint) if !mint.eq_ignore_ascii_case(SOL_MINT) => Some(mint),
        _ => quote_mint.filter(|mint| !mint.eq_ignore_ascii_case(SOL_MINT)),
    };

    if let Some(output_mint) = output_mint {
        return format!("{METEORA_APP_URL}?inputMint={SOL_MINT}&outputMint={output_mint}");
    }

    METEORA_APP_URL.to_string()
}

#[cfg(test)]
mod tests {
    use super::build_meteora_url;
    use crate::api::{LiquidityData, PairData, PriceChange, TokenInfo, VolumeData};

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
        let pair = sample_pair(
            Some("TokenMint12345678901234567890123456789012345"),
            Some("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"),
        );
        let url = build_meteora_url(&pair);
        assert!(url.contains("outputMint=TokenMint12345678901234567890123456789012345"));
    }

    #[test]
    fn build_meteora_url_falls_back_to_home_when_missing_mints() {
        let pair = sample_pair(None, None);
        let url = build_meteora_url(&pair);
        assert_eq!(url, "https://app.meteora.ag/");
    }
}
