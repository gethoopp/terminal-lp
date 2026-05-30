use crate::app::{App, SortColumn, SortDirection};
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
            Constraint::Min(3),    // table
            Constraint::Length(3), // footer
        ])
        .split(frame.area());

    render_header(frame, main_layout[0], app);
    render_table(frame, main_layout[1], app);
    render_footer(frame, main_layout[2], app);
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let title = format!(
        " Volatile Scanner — Solana DEX Pairs  |  Last update: {}  |  {} ",
        app.last_update, app.status_message
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
        let loading = Paragraph::new("Fetching data from DexScreener...")
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading, area);
        return;
    }

    if let Some(ref err) = app.error_message {
        let error = Paragraph::new(format!("Error: {}", err))
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
        Constraint::Length(10), // symbol
        Constraint::Length(12), // price
        Constraint::Length(8),  // 1h %
        Constraint::Length(9),  // 24h %
        Constraint::Length(14), // volume
        Constraint::Length(14), // liquidity
        Constraint::Length(14), // FDV
    ];

    let table = Table::new(rows, widths)
        .header(header_row.style(header_style))
        .block(Block::default().borders(Borders::ALL).title(format!(
            " Pairs ({}) — ↑↓ scroll | ← → sort col | s search mode | +/- vol/liq filter | h help | q quit ",
            app.filtered_pairs.len()
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

    Row::new(vec![
        Cell::from(format!("Symbol{}", sort_marker(SortColumn::Symbol))),
        Cell::from(format!("Price{}", sort_marker(SortColumn::Price))),
        Cell::from(format!("1h%{}", sort_marker(SortColumn::Change1h))),
        Cell::from(format!("24h%{}", sort_marker(SortColumn::Change24h))),
        Cell::from(format!("Vol 24h{}", sort_marker(SortColumn::Volume24h))),
        Cell::from(format!("Liquidity{}", sort_marker(SortColumn::Liquidity))),
        Cell::from(format!("FDV{}", sort_marker(SortColumn::FDV))),
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
            let fdv = pair.fdv.unwrap_or(0.0);

            Row::new(vec![
                Cell::from(symbol.to_uppercase())
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                Cell::from(price),
                Cell::from(change_cell(change_1h)),
                Cell::from(change_cell(change_24h)),
                Cell::from(format_usd(volume)),
                Cell::from(format_usd(liquidity)),
                Cell::from(format_usd(fdv)),
            ])
        })
        .collect()
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

    Text::from(Span::styled(format!("{} {:>6.2}%", arrow, value), style))
}

fn format_price(price: &Option<String>) -> String {
    match price {
        Some(p) => {
            if let Ok(val) = p.parse::<f64>() {
                if val >= 1.0 {
                    format!("${:.4}", val)
                } else if val >= 0.0001 {
                    format!("${:.8}", val)
                } else {
                    format!("${:.12}", val)
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
        format!("${:.2}", value)
    } else {
        String::from("--")
    }
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
        Span::styled("+/-", Style::default().fg(Color::Cyan)),
        Span::raw(" vol filter  "),
        Span::styled("[", Style::default().fg(Color::DarkGray)),
        Span::styled("]", Style::default().fg(Color::DarkGray)),
        Span::raw(" liq filter  "),
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
        Line::from(" Data sourced from DexScreener API — free, public DEX aggregator"),
        Line::from(" Focus: Solana ecosystem (Meteora DLMM, Raydium, Orca, Jupiter)"),
        Line::from(""),
        Line::from(Span::styled(" Controls:", Style::default().fg(Color::Cyan))),
        Line::from("  ↑ / ↓         Navigate pair list"),
        Line::from("  ← / →         Change sort column"),
        Line::from("  r             Reverse sort direction"),
        Line::from("  s             Enter search mode (type token name/symbol, Enter to keep)"),
        Line::from("  Esc           Clear search and exit search mode"),
        Line::from("  + / -         Increase / decrease min volume filter ($1K step)"),
        Line::from("  [ / ]         Decrease / increase min liquidity filter ($500 step)"),
        Line::from("  Enter         Open pair URL in browser (if available)"),
        Line::from("  Ctrl+C        Quit"),
        Line::from("  h             Toggle this help screen"),
        Line::from("  q / Esc       Quit"),
        Line::from(""),
        Line::from(Span::styled(" Columns:", Style::default().fg(Color::Cyan))),
        Line::from("  Symbol        Token ticker"),
        Line::from("  Price         Current price in USD"),
        Line::from("  1h %          1-hour price change (▲ green / ▼ red)"),
        Line::from("  24h %         24-hour price change"),
        Line::from("  Vol 24h       24-hour volume in USD"),
        Line::from("  Liquidity     Total liquidity in USD"),
        Line::from("  FDV           Fully diluted valuation"),
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
