use crate::app::{App, SortColumn, SortDirection, TradeSignal};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

pub fn render(frame: &mut Frame, app: &App) {
    if app.show_help {
        render_help(frame);
        return;
    }

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(10),   // table + analysis
            Constraint::Length(3), // footer
        ])
        .split(frame.area());

    let body_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(6),     // table
            Constraint::Length(12), // analysis
        ])
        .split(main_layout[1]);

    render_header(frame, main_layout[0], app);
    render_table(frame, body_layout[0], app);
    render_analysis(frame, body_layout[1], app);
    render_footer(frame, main_layout[2], app);
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let last_update = &app.last_update;
    let status_message = &app.status_message;
    let title = format!(
        " Volatile Scanner — Solana DEX Pairs  |  Last update: {last_update}  |  {status_message} "
    );

    let header_style = if app.is_loading {
        Style::default().fg(Color::Yellow)
    } else if app.error_message.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Green)
    };

    let header = Paragraph::new(title)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(header_style),
        )
        .style(header_style)
        .alignment(Alignment::Left);

    frame.render_widget(header, area);
}

fn render_table(frame: &mut Frame, area: Rect, app: &App) {
    if app.is_loading {
        let loading = Paragraph::new("Fetching data from DexScreener (Jupiter fallback active)...")
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading, area);
        return;
    }

    if let Some(ref err) = app.error_message {
        let error = Paragraph::new(format!("Error: {err}"))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red)),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Red));
        frame.render_widget(error, area);
        return;
    }

    if app.filtered_pairs.is_empty() {
        let empty = Paragraph::new(
            "No pairs match your filters. Try lowering min volume / liquidity or clearing search.",
        )
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let header_row = build_header(app);
    let rows = build_rows(app);

    let mut table_state = TableState::default().with_selected(Some(app.selected_index));

    let header_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let highlight_style = Style::default()
        .bg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);

    let widths = [
        Constraint::Length(9),  // symbol
        Constraint::Length(12), // price
        Constraint::Length(8),  // 1h %
        Constraint::Length(9),  // RSI
        Constraint::Length(9),  // 24h %
        Constraint::Length(12), // volume
        Constraint::Length(12), // liquidity
        Constraint::Length(12), // mcap
        Constraint::Length(12), // fdv
        Constraint::Length(12), // signal
    ];

    let pair_count = app.filtered_pairs.len();
    let table = Table::new(rows, widths)
        .header(header_row.style(header_style))
        .block(Block::default().borders(Borders::ALL).title(format!(
            " Pairs ({pair_count}) — ↑↓ scroll | ←→ sort | f DEX | w watchlist | W watchlist only | s search | x export CSV | +/-/[ ]/{{ }} filters | h help | q quit "
        )))
        .row_highlight_style(highlight_style)
        .column_spacing(1);

    frame.render_stateful_widget(table, area, &mut table_state);
}

fn build_header(app: &App) -> Row<'static> {
    let sort_marker = |col: SortColumn| -> &str {
        if app.sort_column == col {
            match app.sort_direction {
                SortDirection::Descending => " ▼",
                SortDirection::Ascending => " ▲",
            }
        } else {
            ""
        }
    };

    let symbol_marker = sort_marker(SortColumn::Symbol);
    let price_marker = sort_marker(SortColumn::Price);
    let ch1h_marker = sort_marker(SortColumn::Change1h);
    let rsi_marker = sort_marker(SortColumn::Rsi);
    let ch24h_marker = sort_marker(SortColumn::Change24h);
    let vol_marker = sort_marker(SortColumn::Volume24h);
    let liq_marker = sort_marker(SortColumn::Liquidity);
    let mcap_marker = sort_marker(SortColumn::MarketCap);
    let fdv_marker = sort_marker(SortColumn::Fdv);
    let signal_marker = sort_marker(SortColumn::Signal);

    Row::new(vec![
        Cell::from(format!("Symbol{symbol_marker}")),
        Cell::from(format!("Price{price_marker}")),
        Cell::from(format!("1h%{ch1h_marker}")),
        Cell::from(format!("RSI*{rsi_marker}")),
        Cell::from(format!("24h%{ch24h_marker}")),
        Cell::from(format!("Vol 24h{vol_marker}")),
        Cell::from(format!("Liquidity{liq_marker}")),
        Cell::from(format!("MCap{mcap_marker}")),
        Cell::from(format!("FDV{fdv_marker}")),
        Cell::from(format!("Signal{signal_marker}")),
    ])
}

fn build_rows<'a>(app: &'a App) -> Vec<Row<'a>> {
    app.filtered_pairs
        .iter()
        .map(|&idx| {
            let pair = &app.pairs[idx];
            let symbol = pair.base_token.symbol.as_deref().unwrap_or("???");
            let price = format_price(&pair.price_usd);
            let change_1h = pair
                .price_change
                .as_ref()
                .and_then(|pc| pc.h1)
                .unwrap_or(0.0);
            let change_24h = pair
                .price_change
                .as_ref()
                .and_then(|pc| pc.h24)
                .unwrap_or(0.0);
            let volume = pair.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
            let liquidity = pair.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0);
            let mcap = pair.market_cap.unwrap_or(0.0);
            let fdv = pair.fdv.unwrap_or(0.0);
            let (signal_label, signal_kind, rsi_value) = app
                .assessment_for_pair_index(idx)
                .map(|a| (a.signal.label(), a.signal, a.rsi.as_ref().map(|r| r.value)))
                .unwrap_or(("PANTAU", TradeSignal::Watchlist, None));

            Row::new(vec![
                Cell::from(symbol.to_uppercase())
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                Cell::from(price),
                Cell::from(change_cell(change_1h)),
                Cell::from(rsi_cell(rsi_value)),
                Cell::from(change_cell(change_24h)),
                Cell::from(format_usd(volume)),
                Cell::from(format_usd(liquidity)),
                Cell::from(format_usd(mcap)),
                Cell::from(format_usd(fdv)),
                Cell::from(signal_label).style(signal_style(signal_kind)),
            ])
        })
        .collect()
}

fn signal_style(signal: TradeSignal) -> Style {
    match signal {
        TradeSignal::GoodEntry => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        TradeSignal::Watchlist => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        TradeSignal::Avoid => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    }
}

fn change_cell(value: f64) -> Text<'static> {
    let style = if value > 0.0 {
        Style::default().fg(Color::Green)
    } else if value < 0.0 {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Gray)
    };

    let arrow = if value > 0.0 {
        "▲"
    } else if value < 0.0 {
        "▼"
    } else {
        " "
    };

    Text::from(Span::styled(format!("{arrow} {value:>6.2}%"), style))
}

fn rsi_cell(value: Option<f64>) -> Text<'static> {
    match value {
        Some(rsi) => {
            let style = if rsi >= 70.0 {
                Style::default().fg(Color::Red)
            } else if rsi >= 58.0 {
                Style::default().fg(Color::Green)
            } else if rsi >= 45.0 {
                Style::default().fg(Color::Yellow)
            } else if rsi >= 35.0 {
                Style::default().fg(Color::LightYellow)
            } else {
                Style::default().fg(Color::Cyan)
            };
            Text::from(Span::styled(format!("{rsi:>5.1}"), style))
        }
        None => Text::from(Span::styled("  -- ", Style::default().fg(Color::DarkGray))),
    }
}

fn format_price(price: &Option<String>) -> String {
    match price {
        Some(p) => {
            if let Ok(val) = p.parse::<f64>() {
                if val >= 1.0 {
                    format!("${val:.4}")
                } else if val >= 0.0001 {
                    format!("${val:.8}")
                } else {
                    format!("${val:.12}")
                }
            } else {
                p.chars().take(12).collect()
            }
        }
        None => String::from("--"),
    }
}

fn format_usd(value: f64) -> String {
    if value >= 1_000_000_000.0 {
        format!("${:.2}B", value / 1_000_000_000.0)
    } else if value >= 1_000_000.0 {
        format!("${:.2}M", value / 1_000_000.0)
    } else if value >= 1_000.0 {
        format!("${:.2}K", value / 1_000.0)
    } else if value > 0.0 {
        format!("${value:.2}")
    } else {
        String::from("--")
    }
}

fn render_analysis(frame: &mut Frame, area: Rect, app: &App) {
    let content = if let Some(pair) = app.selected_pair() {
        let assessment = app.selected_assessment();

        let symbol = pair
            .base_token
            .symbol
            .as_deref()
            .unwrap_or("???")
            .to_uppercase();
        let name = pair.base_token.name.as_deref().unwrap_or("-");
        let signal = assessment
            .as_ref()
            .map(|a| a.signal)
            .unwrap_or(TradeSignal::Watchlist);
        let score = assessment.as_ref().map(|a| a.score).unwrap_or(50);
        let bias_summary = assessment
            .as_ref()
            .map(|a| a.bias_summary.as_str())
            .unwrap_or("Data bias pair belum tersedia.");
        let rsi_summary = assessment
            .as_ref()
            .and_then(|a| a.rsi.as_ref())
            .map(|r| r.display())
            .unwrap_or_else(|| "Data tidak cukup".to_string());
        let microstructure_summary = assessment
            .as_ref()
            .map(|a| a.microstructure_summary.as_str())
            .unwrap_or("Data microstructure belum tersedia.");
        let il_estimate = assessment
            .as_ref()
            .and_then(|a| a.il_estimate_24h_pct)
            .map(|il| format!("{il:.2}%"))
            .unwrap_or_else(|| "--".to_string());
        let health_bar = assessment
            .as_ref()
            .map(|a| a.health_bar.as_str())
            .unwrap_or("N/A");
        let notes = assessment
            .as_ref()
            .map(|a| a.notes.clone())
            .unwrap_or_else(|| vec!["Data analisa belum tersedia.".to_string()]);
        let mcap = pair.market_cap.unwrap_or(0.0);
        let fdv = pair.fdv.unwrap_or(0.0);
        let fdv_mcap_ratio = app.selected_fdv_mcap_ratio();
        let shown_notes = notes.iter().take(3).collect::<Vec<_>>();
        let hidden_count = notes.len().saturating_sub(shown_notes.len());
        let is_watchlisted = app.selected_in_watchlist();
        let watchlist_badge = if is_watchlisted { "ON" } else { "OFF" };
        let ratio_text = fdv_mcap_ratio
            .map(|v| format!("{v:.2}x"))
            .unwrap_or_else(|| "--".to_string());
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Screener: ", Style::default().fg(Color::Cyan)),
                Span::styled(symbol, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!(" ({name}) | DEX: {}", pair.dex_id)),
            ]),
            Line::from(vec![
                Span::styled("Watchlist: ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    watchlist_badge,
                    if is_watchlisted {
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
            ]),
            Line::from(vec![
                Span::styled("Rekomendasi: ", Style::default().fg(Color::Cyan)),
                Span::styled(signal.label(), signal_style(signal)),
                Span::raw(format!("  |  Score: {score}/100")),
            ]),
            Line::from(vec![
                Span::styled("Valuasi: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!(
                    "MCap {} | FDV {} | FDV/MCap {}",
                    format_usd(mcap),
                    format_usd(fdv),
                    ratio_text
                )),
            ]),
            Line::from(vec![
                Span::styled("Health Bar: ", Style::default().fg(Color::Cyan)),
                Span::raw(health_bar.to_string()),
            ]),
            Line::from(vec![
                Span::styled("RSI*: ", Style::default().fg(Color::Cyan)),
                Span::raw(rsi_summary),
            ]),
            Line::from(vec![
                Span::styled("IL Est. 24h: ", Style::default().fg(Color::Cyan)),
                Span::raw(il_estimate),
            ]),
            Line::from(vec![
                Span::styled("Bias Pair: ", Style::default().fg(Color::Cyan)),
                Span::raw(bias_summary.to_string()),
            ]),
            Line::from(vec![
                Span::styled("Microstructure: ", Style::default().fg(Color::Cyan)),
                Span::raw(microstructure_summary.to_string()),
            ]),
        ];

        // Yield section
        if let Some(assessment) = assessment.as_ref() {
            if !assessment.yields.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("Yield: ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        assessment.yields.len().to_string() + " pools tersedia",
                        Style::default().fg(Color::Green),
                    ),
                ]));

                for (idx, y) in assessment.yields.iter().take(3).enumerate() {
                    let apy_str = y.display_apy();
                    let dex = y.dex_name.label();
                    let label = if idx == 0 { "  Best:  " } else { "  Alt:   " };
                    lines.push(Line::from(vec![
                        Span::styled(label, Style::default().fg(Color::DarkGray)),
                        Span::raw(format!("{apy_str} APY ({dex})")),
                        Span::raw(if y.il_risk { " | IL Risk" } else { "" }),
                    ]));
                }

                if assessment.yields.len() > 3 {
                    lines.push(Line::from(format!(
                        "  ... +{} pool lainnya",
                        assessment.yields.len() - 3
                    )));
                }
            } else {
                lines.push(Line::from(vec![
                    Span::styled("Yield: ", Style::default().fg(Color::Cyan)),
                    Span::styled("No data", Style::default().fg(Color::DarkGray)),
                ]));
            }
        }

        lines.push(Line::from(vec![
            Span::styled("Bot: ", Style::default().fg(Color::Cyan)),
            Span::raw(app.bot_status.clone()),
        ]));

        if let Some(position) = app.bot_active_position.as_ref() {
            lines.push(Line::from(vec![
                Span::styled("Posisi Aktif: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!(
                    "{} | entry ${:.8} | TP ${:.8} ({:.2}%) | SL ${:.8} ({:.2}%) | buka {}",
                    position.symbol,
                    position.entry_price,
                    position.target_price,
                    position.take_profit_pct,
                    position.stop_loss_price,
                    position.stop_loss_pct,
                    position.opened_at
                )),
            ]));
        }

        for note in shown_notes {
            lines.push(Line::from(format!("- {note}")));
        }
        if hidden_count > 0 {
            lines.push(Line::from(format!("... +{hidden_count} catatan lainnya")));
        }
        lines.push(Line::from(Span::styled(
            "*RSI memakai delta antar-window h24/h6/h1/m5, bukan candle RSI 14 penuh.",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "Heuristik teknikal, bukan financial advice.",
            Style::default().fg(Color::DarkGray),
        )));

        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Analisa Grafik (Heuristik) "),
            )
            .style(Style::default().fg(Color::White))
    } else {
        Paragraph::new("Belum ada pair yang dipilih.")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Analisa Grafik "),
            )
            .style(Style::default().fg(Color::DarkGray))
    };

    frame.render_widget(content, area);
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans = vec![
        Span::styled("← →", Style::default().fg(Color::Cyan)),
        Span::raw(" sort col  "),
        Span::styled("r", Style::default().fg(Color::Cyan)),
        Span::raw(" reverse  "),
        Span::styled("↑↓", Style::default().fg(Color::Cyan)),
        Span::raw(" navigate  "),
        Span::styled("s", Style::default().fg(Color::Cyan)),
        Span::raw(" search  "),
        Span::styled("o", Style::default().fg(Color::Cyan)),
        Span::raw(" open mode  "),
        Span::styled("f", Style::default().fg(Color::Cyan)),
        Span::raw(" DEX filter  "),
        Span::styled("w", Style::default().fg(Color::Cyan)),
        Span::raw(" watchlist  "),
        Span::styled("W", Style::default().fg(Color::Cyan)),
        Span::raw(" watchlist only  "),
        Span::styled("b", Style::default().fg(Color::Cyan)),
        Span::raw(" bot on/off  "),
        Span::styled("p", Style::default().fg(Color::Cyan)),
        Span::raw(" place  "),
        Span::styled("t", Style::default().fg(Color::Cyan)),
        Span::raw(" TP close  "),
        Span::styled("x", Style::default().fg(Color::Cyan)),
        Span::raw(" export CSV  "),
        Span::styled("+/-", Style::default().fg(Color::Cyan)),
        Span::raw(" vol filter  "),
        Span::styled("[", Style::default().fg(Color::DarkGray)),
        Span::styled("]", Style::default().fg(Color::DarkGray)),
        Span::raw(" liq filter  "),
        Span::styled("{", Style::default().fg(Color::DarkGray)),
        Span::styled("}", Style::default().fg(Color::DarkGray)),
        Span::raw(" mcap filter  "),
        Span::styled("h", Style::default().fg(Color::Cyan)),
        Span::raw(" help  "),
        Span::styled("q", Style::default().fg(Color::Cyan)),
        Span::raw(" quit"),
    ];

    // Show current filter values
    spans.push(Span::raw("  |  Vol ≥ "));
    spans.push(Span::styled(
        format_usd(app.min_volume_filter),
        Style::default().fg(Color::Yellow),
    ));
    spans.push(Span::raw("  Liq ≥ "));
    spans.push(Span::styled(
        format_usd(app.min_liquidity_filter),
        Style::default().fg(Color::Yellow),
    ));
    spans.push(Span::raw("  MCap ≥ "));
    spans.push(Span::styled(
        format_usd(app.min_market_cap_filter),
        Style::default().fg(Color::Yellow),
    ));
    spans.push(Span::raw("  DEX: "));
    spans.push(Span::styled(
        app.dex_filter.label(),
        Style::default().fg(Color::Yellow),
    ));
    spans.push(Span::raw("  WL only: "));
    spans.push(Span::styled(
        if app.watchlist_only { "ON" } else { "OFF" },
        if app.watchlist_only {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        },
    ));
    spans.push(Span::raw("  BOT: "));
    spans.push(Span::styled(
        if app.bot_enabled { "ON" } else { "OFF" },
        if app.bot_enabled {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        },
    ));

    if !app.search_query.is_empty() {
        spans.push(Span::raw("  Search: "));
        spans.push(Span::styled(
            app.search_query.clone(),
            Style::default().fg(Color::Green),
        ));
    }

    if app.search_mode {
        spans.push(Span::raw("  |  "));
        spans.push(Span::styled(
            "SEARCH MODE (Enter keep / Esc clear)",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }

    if app.open_link_mode {
        spans.push(Span::raw("  |  "));
        spans.push(Span::styled(
            "OPEN MODE (M/Enter Meteora, D DexScreener, Esc cancel)",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let footer = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Gray));

    frame.render_widget(footer, area);
}

fn render_help(frame: &mut Frame) {
    let help_text = vec![
        Line::from(Span::styled(
            " Volatile Scanner — Help ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(" Data source: DexScreener API + Jupiter quote fallback"),
        Line::from(" Focus: Solana ecosystem (Meteora DLMM, Raydium, Orca, Jupiter)"),
        Line::from(""),
        Line::from(Span::styled(" Controls:", Style::default().fg(Color::Cyan))),
        Line::from("  ↑ / ↓         Navigate pair list"),
        Line::from("  ← / →         Change sort column"),
        Line::from("  r             Reverse sort direction"),
        Line::from("  f             Cycle DEX filter (All/Meteora/Raydium/Orca/PumpSwap)"),
        Line::from("  s             Enter search mode (type token name/symbol, Enter to keep)"),
        Line::from("  w             Toggle selected pair in watchlist (persisted local JSON)"),
        Line::from("  W             Toggle watchlist-only mode"),
        Line::from("  b             Toggle bot mode ON/OFF"),
        Line::from("  p             Place position via local Meteora bot API"),
        Line::from("  t             Manual close for active bot position"),
        Line::from("  o             Open mode: pilih tujuan link pair"),
        Line::from("  x             Export hasil pair + assessment ke CSV"),
        Line::from("  Esc           Clear search and exit search mode"),
        Line::from("  + / -         Increase / decrease min volume filter ($1K step)"),
        Line::from("  [ / ]         Decrease / increase min liquidity filter ($500 step)"),
        Line::from("  { / }         Decrease / increase min market cap filter ($100K step)"),
        Line::from("  Enter         Open pair in Meteora"),
        Line::from("  m             (di Open mode) buka Meteora"),
        Line::from("  d             (di Open mode) buka DexScreener"),
        Line::from("  Config file   ~/.config/haliim-scanner/config.toml"),
        Line::from("  Log file      ~/.local/share/haliim-scanner/scanner.log"),
        Line::from("  Ctrl+C        Quit"),
        Line::from("  h             Toggle this help screen"),
        Line::from("  q / Esc       Quit"),
        Line::from(""),
        Line::from(Span::styled(" Columns:", Style::default().fg(Color::Cyan))),
        Line::from("  Symbol        Token ticker"),
        Line::from("  Price         Current price in USD"),
        Line::from("  1h %          1-hour price change (▲ green / ▼ red)"),
        Line::from("  RSI*          RSI heuristik dari delta h24/h6/h1/m5 untuk arah entry LP"),
        Line::from("  24h %         24-hour price change"),
        Line::from("  Vol 24h       24-hour volume in USD"),
        Line::from("  Liquidity     Total liquidity in USD"),
        Line::from("  MCap          Market capitalization"),
        Line::from("  FDV           Fully diluted valuation"),
        Line::from("  Signal        Rekomendasi heuristik (Layak Masuk / Pantau / Hindari)"),
        Line::from(""),
        Line::from(Span::styled(
            " Auto-refresh: every 15 seconds",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(" Press 'h' to return to scanner"),
    ];

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Help ")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(help, frame.area());
}

#[cfg(test)]
mod tests {
    use super::{format_price, format_usd};

    #[test]
    fn format_price_handles_ranges() {
        assert_eq!(format_price(&Some("12.34567".to_string())), "$12.3457");
        assert_eq!(
            format_price(&Some("0.123456789".to_string())),
            "$0.12345679"
        );
        assert_eq!(format_price(&None), "--");
    }

    #[test]
    fn format_usd_uses_compact_suffixes() {
        assert_eq!(format_usd(1_500.0), "$1.50K");
        assert_eq!(format_usd(2_500_000.0), "$2.50M");
        assert_eq!(format_usd(0.0), "--");
    }
}
