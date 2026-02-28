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
    let alert = Paragraph::new(Line::from(vec![
        Span::styled("> alert -- ", Style::default().fg(Color::Cyan)),
        Span::styled(&app.alert_line, alert_style),
    ]))
    .style(Style::default().bg(Color::Black));
    f.render_widget(alert, area);
}

fn draw_prompt(f: &mut Frame, area: Rect, app: &App) {
    let mode_label = match app.mode {
        Mode::Alert => "> alert ",
        Mode::Scan => "> scan  ",
    };
    let prompt = Paragraph::new(Line::from(vec![
        Span::styled(mode_label, Style::default().fg(Color::Cyan)),
        Span::raw(&app.input),
    ]))
    .style(Style::default().bg(Color::Black));
    f.render_widget(prompt, area);

    // Set cursor position in scan mode
    if app.mode == Mode::Scan {
        f.set_cursor_position((
            area.x + mode_label.len() as u16 + app.input_cursor as u16,
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
                    Constraint::Percentage(50), // output (left)
                    Constraint::Percentage(50), // alert table (right)
                ])
                .split(area);
            draw_output(f, chunks[0], app);
            draw_alert_table(f, chunks[1], app);
        }
        Mode::Scan => draw_output(f, area, app),
    }
}

fn draw_alert_table(f: &mut Frame, area: Rect, app: &App) {
    if app.alert_rows.is_empty() {
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
            let name = r.name.as_deref().unwrap_or("");
            let name = if name.len() > 24 {
                format!("{}..", &name[..22])
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
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(5),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .style(Style::default().bg(Color::Black));

    f.render_widget(table, area);
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
