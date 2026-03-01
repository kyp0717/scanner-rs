use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};

use super::app::{App, Mode};

/// Draw the TUI layout.
pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title
            Constraint::Length(1), // alert line
            Constraint::Length(1), // prompt
            Constraint::Min(5),   // main area
        ])
        .split(f.area());

    draw_title(f, chunks[0], app);
    draw_alert_line(f, chunks[1], app);
    draw_prompt(f, chunks[2], app);
    draw_main(f, chunks[3], app);
}

fn draw_title(f: &mut Frame, area: Rect, app: &App) {
    let title = Paragraph::new(Line::from(Span::styled(
        &app.title,
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Center)
    .style(Style::default().bg(Color::Black));
    f.render_widget(title, area);
}

fn draw_alert_line(f: &mut Frame, area: Rect, app: &App) {
    let alert_style = if app.alert_line.contains("ALERT") {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let alert = Paragraph::new(Line::from(Span::styled(&app.alert_line, alert_style)))
        .style(Style::default().bg(Color::Black));
    f.render_widget(alert, area);
}

fn draw_prompt(f: &mut Frame, area: Rect, app: &App) {
    let prompt = match app.mode {
        Mode::Alert => Paragraph::new(Line::from(Span::styled(
            " Insert=scan  Up/Down=navigate  Esc=back",
            Style::default().fg(Color::DarkGray),
        )))
        .style(Style::default().bg(Color::Black)),
        Mode::Scan => Paragraph::new(Line::from(vec![
            Span::styled("> scan  ", Style::default().fg(Color::Cyan)),
            Span::raw(&app.input),
        ]))
        .style(Style::default().bg(Color::Black)),
    };
    f.render_widget(prompt, area);

    // Set cursor position in scan mode
    if app.mode == Mode::Scan {
        let prompt_len = "> scan  ".len();
        f.set_cursor_position((
            area.x + prompt_len as u16 + app.input_cursor as u16,
            area.y,
        ));
    }
}

fn draw_main(f: &mut Frame, area: Rect, app: &App) {
    match app.mode {
        Mode::Alert => {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(30), // output log
                    Constraint::Percentage(35), // alert table
                    Constraint::Percentage(35), // detail panel
                ])
                .split(area);
            draw_output(f, chunks[0], app);
            draw_alert_table(f, chunks[1], app);
            draw_detail_panel(f, chunks[2], app);
        }
        Mode::Scan => draw_output(f, area, app),
    }
}

fn draw_alert_table(f: &mut Frame, area: Rect, app: &App) {
    if app.engine.alert_rows.is_empty() {
        let msg = Paragraph::new("No alerts yet. Press Insert for scan mode.")
            .style(Style::default().fg(Color::DarkGray).bg(Color::Black));
        f.render_widget(msg, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("Time"),
        Cell::from("Symbol"),
        Cell::from("Chg%"),
        Cell::from("Last"),
        Cell::from("Hits"),
        Cell::from("Name"),
    ])
    .style(
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = app
        .engine
        .alert_rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let chg_str = r
                .change_pct
                .map(|c| format!("{c:+.1}%"))
                .unwrap_or("-".into());
            let price = r.last.map(|p| format!("{p:.2}")).unwrap_or("-".into());
            let hits = format!("{}/8", r.scanner_hits);
            let name = if r.enriched {
                r.name.as_deref().unwrap_or("-")
            } else {
                "..."
            };
            let name = if name.len() > 20 {
                format!("{}..", &name[..18])
            } else {
                name.to_string()
            };

            let style = if i == app.selected_alert_row {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(r.alert_time.as_str()),
                Cell::from(Span::styled(
                    r.symbol.as_str(),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
                Cell::from(Span::styled(
                    chg_str,
                    if r.change_pct.unwrap_or(0.0) >= 0.0 {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Red)
                    },
                )),
                Cell::from(price),
                Cell::from(hits),
                Cell::from(name),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(9),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(4),
            Constraint::Min(10),
        ],
    )
    .header(header)
    .style(Style::default().bg(Color::Black));

    f.render_widget(table, area);
}

fn draw_detail_panel(f: &mut Frame, area: Rect, app: &App) {
    let dim = Style::default().fg(Color::DarkGray);
    let label_style = Style::default().fg(Color::Yellow);
    let val_style = Style::default().fg(Color::White);

    if app.engine.alert_rows.is_empty() || app.selected_alert_row >= app.engine.alert_rows.len() {
        let msg = Paragraph::new("No stock selected")
            .style(Style::default().fg(Color::DarkGray).bg(Color::Black));
        f.render_widget(msg, area);
        return;
    }

    let r = &app.engine.alert_rows[app.selected_alert_row];
    let mut lines: Vec<Line> = Vec::new();

    // Symbol header
    lines.push(Line::from(Span::styled(
        &r.symbol,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Helper closures
    let fmt_or_dots = |val: &Option<String>| -> Span {
        match val {
            Some(v) if !v.is_empty() => Span::styled(v.clone(), val_style),
            _ if r.enriched => Span::styled("-", dim),
            _ => Span::styled("...", dim),
        }
    };

    let fmt_f64 = |val: Option<f64>, fmt: &dyn Fn(f64) -> String| -> Span {
        match val {
            Some(v) => Span::styled(fmt(v), val_style),
            None if r.enriched => Span::styled("-", dim),
            None => Span::styled("...", dim),
        }
    };

    // Price
    lines.push(Line::from(vec![
        Span::styled("Price     ", label_style),
        match r.last {
            Some(p) => Span::styled(format!("${p:.2}"), val_style),
            None => Span::styled("-", dim),
        },
    ]));

    // Change%
    lines.push(Line::from(vec![
        Span::styled("Change    ", label_style),
        match r.change_pct {
            Some(c) => Span::styled(
                format!("{c:+.1}%"),
                if c >= 0.0 {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                },
            ),
            None => Span::styled("-", dim),
        },
    ]));

    // Volume
    lines.push(Line::from(vec![
        Span::styled("Volume    ", label_style),
        match r.volume {
            Some(v) => Span::styled(format_volume(v), val_style),
            None => Span::styled("-", dim),
        },
    ]));

    // RVol
    lines.push(Line::from(vec![
        Span::styled("RVol      ", label_style),
        fmt_f64(r.rvol, &|v| format!("{v:.1}x")),
    ]));

    // Float
    lines.push(Line::from(vec![
        Span::styled("Float     ", label_style),
        fmt_f64(r.float_shares, &|v| {
            if v >= 1_000_000_000.0 {
                format!("{:.1}B", v / 1_000_000_000.0)
            } else if v >= 1_000_000.0 {
                format!("{:.1}M", v / 1_000_000.0)
            } else if v >= 1_000.0 {
                format!("{:.0}K", v / 1_000.0)
            } else {
                format!("{v:.0}")
            }
        }),
    ]));

    // Short%
    lines.push(Line::from(vec![
        Span::styled("Short%    ", label_style),
        fmt_f64(r.short_pct, &|v| format!("{:.1}%", v * 100.0)),
    ]));

    lines.push(Line::from(""));

    // Name
    lines.push(Line::from(vec![
        Span::styled("Name      ", label_style),
        fmt_or_dots(&r.name),
    ]));

    // Sector
    lines.push(Line::from(vec![
        Span::styled("Sector    ", label_style),
        fmt_or_dots(&r.sector),
    ]));

    // Industry
    lines.push(Line::from(vec![
        Span::styled("Industry  ", label_style),
        fmt_or_dots(&r.industry),
    ]));

    lines.push(Line::from(""));

    // Scanner Hits
    lines.push(Line::from(vec![
        Span::styled("Scanners  ", label_style),
        Span::styled(format!("{}/8", r.scanner_hits), val_style),
    ]));

    // Catalyst
    let catalyst_display = r.catalyst.as_ref().map(|cat| {
        let ago = r.catalyst_time.map(|epoch| {
            let now = chrono::Utc::now().timestamp();
            let diff = now - epoch;
            if diff < 60 { "now".to_string() }
            else if diff < 3600 { format!("{}m ago", diff / 60) }
            else if diff < 86400 { format!("{}h ago", diff / 3600) }
            else { format!("{}d ago", diff / 86400) }
        });
        match ago {
            Some(a) => format!("{cat} ({a})"),
            None => cat.clone(),
        }
    });
    lines.push(Line::from(vec![
        Span::styled("Catalyst  ", label_style),
        fmt_or_dots(&catalyst_display),
    ]));

    // News Headlines
    if !r.news_headlines.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("News", label_style)));
        for (i, headline) in r.news_headlines.iter().take(5).enumerate() {
            let ago = headline.published
                .map(|epoch| {
                    let now = chrono::Utc::now().timestamp();
                    let diff = now - epoch;
                    if diff < 60 { "now".to_string() }
                    else if diff < 3600 { format!("{}m", diff / 60) }
                    else if diff < 86400 { format!("{}h", diff / 3600) }
                    else { format!("{}d", diff / 86400) }
                })
                .unwrap_or_default();
            let prefix = if ago.is_empty() {
                format!(" {}. ", i + 1)
            } else {
                format!(" {}. {} ", i + 1, ago)
            };
            let title = &headline.title;
            let max_title = 50usize.saturating_sub(prefix.len());
            let truncated = if title.len() > max_title {
                format!("{}...", &title[..max_title.saturating_sub(3)])
            } else {
                title.clone()
            };
            lines.push(Line::from(Span::styled(
                format!("{prefix}{truncated}"),
                Style::default().fg(Color::White),
            )));
        }
    } else if !r.enriched {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("News      ", label_style),
            Span::styled("...", dim),
        ]));
    }

    let detail = Paragraph::new(lines)
        .style(Style::default().bg(Color::Black));
    f.render_widget(detail, area);
}

fn format_volume(vol: i64) -> String {
    if vol >= 1_000_000 {
        format!("{:.1}M", vol as f64 / 1_000_000.0)
    } else if vol >= 1_000 {
        format!("{:.0}K", vol as f64 / 1_000.0)
    } else {
        format!("{vol}")
    }
}

fn draw_output(f: &mut Frame, area: Rect, app: &App) {
    let lines: Vec<Line> = app
        .output_lines
        .iter()
        .map(|l| Line::from(l.as_str()))
        .collect();

    let output = Paragraph::new(lines)
        .style(Style::default().fg(Color::White).bg(Color::Black))
        .scroll((app.scroll_offset, 0));

    f.render_widget(output, area);
}
