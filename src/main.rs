mod api;
mod app;
mod ui;

use app::App;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::DefaultTerminal;
use std::time::Duration;

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

    // Initial data fetch
    fetch_and_update(&mut app).await;

    let tick_rate = Duration::from_millis(200);
    let refresh_rate = Duration::from_secs(15);
    let mut last_refresh = std::time::Instant::now();

    loop {
        terminal.draw(|frame| ui::render(frame, &app))?;

        // Auto-refresh
        if last_refresh.elapsed() >= refresh_rate && !app.show_help {
            app.is_loading = true;
            terminal.draw(|frame| ui::render(frame, &app))?;
            fetch_and_update(&mut app).await;
            last_refresh = std::time::Instant::now();
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

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('h') => app.toggle_help(),
                    KeyCode::Up | KeyCode::Char('k') => app.move_selection_up(),
                    KeyCode::Down | KeyCode::Char('j') => {
                        let visible = terminal
                            .size()?
                            .height
                            .saturating_sub(7) as usize; // header + footer + borders
                        app.move_selection_down(visible.max(5));
                    }
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
                                let _ = std::process::Command::new("open")
                                    .arg(url)
                                    .spawn();
                            }
                        }
                    }
                    KeyCode::Char('s') => {
                        // Simple search: read next chars as query
                        app.search_query.clear();
                        app.apply_filters();
                        terminal.draw(|frame| ui::render(frame, &app))?;

                        // Read search input
                        loop {
                            if event::poll(Duration::from_millis(500))? {
                                if let Event::Key(k) = event::read()? {
                                    if k.kind != KeyEventKind::Press {
                                        continue;
                                    }
                                    match k.code {
                                        KeyCode::Enter => break,
                                        KeyCode::Esc => {
                                            app.search_query.clear();
                                            break;
                                        }
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
                                    terminal.draw(|frame| ui::render(frame, &app))?;
                                }
                            } else {
                                break; // timeout
                            }
                        }
                        terminal.draw(|frame| ui::render(frame, &app))?;
                    }
                    KeyCode::Char('R') => {
                        // Force refresh
                        app.is_loading = true;
                        terminal.draw(|frame| ui::render(frame, &app))?;
                        fetch_and_update(&mut app).await;
                        last_refresh = std::time::Instant::now();
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

async fn fetch_and_update(app: &mut App) {
    app.is_loading = true;
    match api::fetch_volatile_pairs().await {
        Ok(pairs) => {
            app.update_pairs(pairs);
        }
        Err(e) => {
            app.set_error(e);
        }
    }
}
