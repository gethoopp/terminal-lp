mod api;
mod app;
mod ui;

use app::App;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{self, error::TryRecvError, UnboundedReceiver, UnboundedSender};

type FetchMessage = Result<api::FetchResult, String>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();
    terminal.clear()?;

    let result = run_app(&mut terminal).await;

    ratatui::restore();
    result
}

async fn run_app(terminal: &mut DefaultTerminal) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    let client = api::build_http_client().map_err(std::io::Error::other)?;
    let (fetch_tx, mut fetch_rx) = mpsc::unbounded_channel::<FetchMessage>();
    let mut fetch_in_flight = false;

    // Initial data fetch in background to keep UI responsive.
    request_fetch(&client, &fetch_tx, &mut fetch_in_flight, &mut app);

    let tick_rate = Duration::from_millis(200);
    let refresh_rate = Duration::from_secs(15);
    let mut last_refresh = Instant::now();

    loop {
        drain_fetch_results(&mut fetch_rx, &mut app, &mut fetch_in_flight);
        terminal.draw(|frame| ui::render(frame, &app))?;

        // Auto-refresh
        if last_refresh.elapsed() >= refresh_rate && !app.show_help && !fetch_in_flight {
            request_fetch(&client, &fetch_tx, &mut fetch_in_flight, &mut app);
            last_refresh = Instant::now();
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
                    KeyCode::Enter => {
                        if let Some(pair) = app.selected_pair() {
                            if let Some(ref url) = pair.url {
                                if let Err(err) = open::that(url) {
                                    app.set_warning(format!("Failed to open URL: {}", err));
                                }
                            }
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
                            app.set_warning(format!(
                                "Loaded with warnings: {}",
                                data.warnings.join(" | ")
                            ));
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
