use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tracing::info;

use crate::config::SupabaseConfig;
use crate::engine::{AlertEngine, EngineEvent};
use crate::history::SupabaseClient;
use crate::models::*;
use crate::tws;
use super::ui;

/// Application mode.
#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Alert,
    Scan,
}

/// Application state for the TUI — thin wrapper around AlertEngine.
pub struct App {
    pub engine: AlertEngine,
    // TUI-only state
    pub mode: Mode,
    pub output_lines: Vec<String>,
    pub alert_line: String,
    pub title: String,
    pub input: String,
    pub input_cursor: usize,
    pub command_history: Vec<String>,
    pub history_idx: i32,
    pub should_quit: bool,
    pub selected_alert_row: usize,
    pub scroll_offset: u16,
}

impl App {
    pub fn new(engine: AlertEngine) -> Self {
        Self {
            engine,
            mode: Mode::Alert,
            output_lines: Vec::new(),
            alert_line: "No alerts".to_string(),
            title: "Scanner REPL -- type help for commands".to_string(),
            input: String::new(),
            input_cursor: 0,
            command_history: Vec::new(),
            history_idx: -1,
            should_quit: false,
            selected_alert_row: 0,
            scroll_offset: 0,
        }
    }

    fn update_title(&mut self) {
        let port = self
            .engine
            .connected_port
            .or(self.engine.settings.port)
            .map(|p| p.to_string())
            .unwrap_or_else(|| "auto".to_string());
        let mode_tag = match self.mode {
            Mode::Alert => "[ALERT]",
            Mode::Scan => "[SCAN]",
        };
        self.title = format!(
            "Scanner REPL -- {}:{} {}",
            self.engine.settings.host, port, mode_tag
        );
    }

    fn push_output(&mut self, line: &str) {
        self.output_lines.push(line.to_string());
    }

    fn clear_output(&mut self) {
        self.output_lines.clear();
        self.scroll_offset = 0;
    }

    fn record_command(&mut self, cmd: &str) {
        if !cmd.is_empty()
            && (self.command_history.is_empty() || self.command_history.last().unwrap() != cmd)
        {
            self.command_history.push(cmd.to_string());
        }
        self.history_idx = -1;
    }

    pub fn handle_input(&mut self, line: &str, rt: &tokio::runtime::Handle) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }

        self.record_command(line);
        let parts: Vec<&str> = line.split_whitespace().collect();
        let cmd = parts[0].to_lowercase();
        let args = &parts[1..];

        self.clear_output();

        match cmd.as_str() {
            "quit" | "exit" | "q" => self.should_quit = true,
            "help" => self.cmd_help(),
            "scan" => self.cmd_scan(args),
            "list" => self.cmd_list(args),
            "set" => self.cmd_set(args),
            "show" => self.cmd_show(),
            "aliases" => self.cmd_aliases(),
            "poll" => self.cmd_poll(args, rt),
            "history" => self.cmd_history(args, rt),
            "mode" => self.cmd_mode(args),
            _ => {
                self.push_output(&format!("Unknown command: {cmd} -- type help"));
            }
        }
    }

    fn cmd_help(&mut self) {
        let help = [
            "Commands:",
            "  scan <alias|code> [--rows N] [--min-price N] [--max-price N]",
            "  list                  Show scanner groups",
            "  list <group>          Expand group (fuzzy match)",
            "  poll                  Show polling status",
            "  poll on|off           Start/stop background polling",
            "  poll clear            Clear seen-set (re-alert)",
            "  history               Show today's tracked stocks",
            "  history all           Show all historical stocks",
            "  history clear         Clear entire history",
            "  set <key> <value>     Change setting",
            "  show                  Current settings",
            "  aliases               Alias map",
            "  help                  This help",
            "  quit / exit / q       Exit",
            "",
            "Settings: port, host, rows, minprice, maxprice",
        ];
        for line in help {
            self.push_output(line);
        }
    }

    fn cmd_scan(&mut self, args: &[&str]) {
        if args.is_empty() {
            self.push_output(
                "Usage: scan <alias|code> [--rows N] [--min-price N] [--max-price N]",
            );
            return;
        }

        if self.engine.bg_busy {
            self.push_output("Background operation in progress, please wait...");
            return;
        }

        let scanner_code = resolve_scanner(args[0]);
        let mut rows = self.engine.settings.rows;
        let mut min_price = self.engine.settings.min_price;
        let mut max_price = self.engine.settings.max_price;

        let mut i = 1;
        while i < args.len() {
            match args[i] {
                "--rows" if i + 1 < args.len() => {
                    rows = args[i + 1].parse().unwrap_or(rows);
                    i += 2;
                }
                "--min-price" if i + 1 < args.len() => {
                    min_price = args[i + 1].parse().ok();
                    i += 2;
                }
                "--max-price" if i + 1 < args.len() => {
                    max_price = args[i + 1].parse().ok();
                    i += 2;
                }
                other => {
                    self.push_output(&format!("Unknown option: {other}"));
                    return;
                }
            }
        }

        self.mode = Mode::Scan;
        self.update_title();
        self.push_output(&format!("Scanning {scanner_code} (rows={rows})..."));
        self.alert_line = format!("Scanning {scanner_code}...");

        self.engine.start_scan(&scanner_code, rows, min_price, max_price);
    }

    fn cmd_list(&mut self, args: &[&str]) {
        if self.engine.bg_busy {
            self.push_output("Background operation in progress, please wait...");
            return;
        }

        let group = if args.is_empty() {
            None
        } else {
            Some(args.join(" "))
        };

        self.push_output("Fetching scanner groups...");
        self.engine.start_list(group);
    }

    fn cmd_poll(&mut self, args: &[&str], _rt: &tokio::runtime::Handle) {
        if args.is_empty() {
            let status = if self.engine.polling { "on" } else { "off" };
            self.push_output(&format!(
                "  Polling: {}  |  Seen: {} symbols",
                status,
                self.engine.alert_seen.len()
            ));
            return;
        }

        match args[0].to_lowercase().as_str() {
            "on" => {
                if self.engine.polling {
                    self.push_output("Polling already active");
                    return;
                }
                let _ = self.engine.poll_on();
                self.push_output("Polling started -- scanning every 60s");
                self.alert_line = "Polling active".to_string();
            }
            "off" => {
                self.engine.poll_off();
                self.push_output("Polling stopped");
                self.alert_line = "Polling stopped".to_string();
            }
            "clear" => {
                let count = self.engine.poll_clear();
                self.push_output(&format!(
                    "Cleared {count} seen symbols and alert table"
                ));
            }
            _ => {
                self.push_output("Usage: poll [on|off|clear]");
            }
        }
    }

    fn cmd_history(&mut self, args: &[&str], rt: &tokio::runtime::Handle) {
        let db = match &self.engine.db {
            Some(db) => db,
            None => {
                self.push_output("Supabase not connected");
                return;
            }
        };

        if args.first().map(|s| s.to_lowercase()) == Some("clear".to_string()) {
            let count = rt.block_on(db.clear_history()).unwrap_or(0);
            self.push_output(&format!("Cleared {count} stocks from history"));
            return;
        }

        let (stocks, label) =
            if args.first().map(|s| s.to_lowercase()) == Some("all".to_string()) {
                (
                    rt.block_on(db.get_history(500)).unwrap_or_default(),
                    "All history",
                )
            } else if let Some(n) = args.first().and_then(|s| s.parse::<u32>().ok()) {
                (
                    rt.block_on(db.get_history(n)).unwrap_or_default(),
                    "Last N",
                )
            } else {
                (rt.block_on(db.get_today()).unwrap_or_default(), "Today")
            };

        if stocks.is_empty() {
            self.push_output(&format!("{label}: no stocks in history"));
            return;
        }

        self.push_output(&format!("{label} -- {} stocks", stocks.len()));
        self.push_output(&format!(
            "{:<10}  {:<6}  {:>8}  {:>8}  {:>6}  {:<30}  {:>4}  {}",
            "Time", "Symbol", "Last", "Chg%", "RVol", "Scanners", "Hits", "Catalyst"
        ));
        self.push_output(&"-".repeat(100));

        for s in &stocks {
            let time_str = crate::history::local_time_str(&s.first_seen);
            let price = s
                .last_price
                .map(|p| format!("{p:.2}"))
                .unwrap_or("-".into());
            let chg = s
                .change_pct
                .map(|c| format!("{c:+.1}%"))
                .unwrap_or("-".into());
            let rvol = s
                .rvol
                .map(|r| format!("{r:.1}x"))
                .unwrap_or("-".into());
            let hits = s.hit_count.unwrap_or(0);
            let cat = s.catalyst.as_deref().unwrap_or("");
            let cat = if cat.len() > 30 {
                format!("{}..", &cat[..28])
            } else {
                cat.to_string()
            };
            self.push_output(&format!(
                "{:<10}  {:<6}  {:>8}  {:>8}  {:>6}  {:<30}  {:>4}  {}",
                time_str, s.symbol, price, chg, rvol, s.scanners, hits, cat
            ));
        }
    }

    fn cmd_set(&mut self, args: &[&str]) {
        if args.len() < 2 {
            self.push_output("Usage: set <key> <value>");
            self.push_output("Keys: port, host, rows, minprice, maxprice");
            return;
        }

        let key = args[0].to_lowercase();
        let val = args[1];

        match key.as_str() {
            "host" => self.engine.settings.host = val.to_string(),
            "port" => {
                self.engine.settings.port = val.parse().ok();
            }
            "rows" => {
                self.engine.settings.rows = val.parse().unwrap_or(self.engine.settings.rows);
            }
            "minprice" => {
                self.engine.settings.min_price = val.parse().ok();
            }
            "maxprice" => {
                self.engine.settings.max_price = if val.to_lowercase() == "none" {
                    None
                } else {
                    val.parse().ok()
                };
            }
            _ => {
                self.push_output(&format!("Unknown setting: {key}"));
                return;
            }
        }

        self.push_output(&format!("  {key} = {val}"));
        self.update_title();
    }

    fn cmd_show(&mut self) {
        self.push_output("Settings:");
        self.push_output(&format!(
            "  port      = {}",
            self.engine
                .settings
                .port
                .map(|p| p.to_string())
                .unwrap_or("auto".to_string())
        ));
        self.push_output(&format!(
            "  host      = {}",
            self.engine.settings.host
        ));
        self.push_output(&format!(
            "  rows      = {}",
            self.engine.settings.rows
        ));
        self.push_output(&format!(
            "  minprice  = {}",
            self.engine
                .settings
                .min_price
                .map(|p| p.to_string())
                .unwrap_or("none".to_string())
        ));
        self.push_output(&format!(
            "  maxprice  = {}",
            self.engine
                .settings
                .max_price
                .map(|p| p.to_string())
                .unwrap_or("none".to_string())
        ));
    }

    fn cmd_aliases(&mut self) {
        self.push_output("Scanner Aliases:");
        for (alias, code) in ALIASES {
            self.push_output(&format!("  {alias:<10}  {code}"));
        }
    }

    fn cmd_mode(&mut self, args: &[&str]) {
        if args.is_empty() {
            let mode_str = match self.mode {
                Mode::Alert => "alert",
                Mode::Scan => "scan",
            };
            self.push_output(&format!("  Mode: {mode_str}"));
            return;
        }
        match args[0].to_lowercase().as_str() {
            "alert" => {
                self.mode = Mode::Alert;
                self.update_title();
            }
            "scan" => {
                self.mode = Mode::Scan;
                self.update_title();
            }
            _ => {
                self.push_output("Usage: mode [alert|scan]");
            }
        }
    }

    /// Translate engine events into TUI state updates.
    fn handle_engine_event(&mut self, event: EngineEvent) {
        match event {
            EngineEvent::ScanComplete {
                scanner_code,
                results,
            } => {
                self.clear_output();
                if results.is_empty() {
                    self.push_output("No results.");
                    self.alert_line = format!("{scanner_code} -- 0 results");
                } else {
                    self.push_output(&format!(
                        "{:>3}  {:<6}  {:>8}  {:>8}  {:>12}  {:>6}  {:>8}  {:>7}  {:<20}  {:<14}  {}",
                        "#", "Symbol", "Last", "Chg%", "Volume", "RVol", "Float", "Short%", "Name", "Sector", "Catalyst"
                    ));
                    self.push_output(&"-".repeat(120));

                    for r in &results {
                        use crate::scanner::*;
                        let name = r.name.as_deref().unwrap_or("-");
                        let sector = r.sector.as_deref().unwrap_or("-");
                        let catalyst = r.catalyst.as_deref().unwrap_or("");
                        self.push_output(&format!(
                            "{:>3}  {:<6}  {:>8}  {:>8}  {:>12}  {:>6}  {:>8}  {:>7}  {:<20}  {:<14}  {}",
                            r.rank,
                            r.symbol,
                            fmt_price(r.last),
                            fmt_change_pct(r.change_pct),
                            fmt_volume(r.volume),
                            fmt_rvol(r.rvol),
                            fmt_float(r.float_shares),
                            fmt_short_pct(r.short_pct),
                            truncate(name, 20),
                            truncate(sector, 14),
                            truncate(catalyst, 30),
                        ));
                    }
                    self.push_output(&format!("\nTotal: {} stocks", results.len()));
                    let now = chrono::Local::now().format("%H:%M:%S");
                    self.alert_line =
                        format!("[{now}] {scanner_code} -- {} results", results.len());
                }
            }
            EngineEvent::ListComplete { xml, group } => {
                self.clear_output();
                match xml {
                    Some(xml) => {
                        let tree = tws::group_scans(&xml);
                        let total: usize = tree
                            .values()
                            .flat_map(|cats| cats.values().map(|s| s.len()))
                            .sum();

                        if let Some(query) = group {
                            let query_lower = query.to_lowercase();
                            for inst in tree.keys() {
                                for (cat, entries) in &tree[inst] {
                                    if cat.to_lowercase().contains(&query_lower) {
                                        self.push_output(&format!(
                                            "{inst} > {cat} ({} scanners)",
                                            entries.len()
                                        ));
                                        self.push_output(&format!(
                                            "{:<30}  {}",
                                            "Scanner Code", "Description"
                                        ));
                                        self.push_output(&"-".repeat(60));
                                        let mut sorted = entries.clone();
                                        sorted.sort_by(|a, b| a.1.cmp(&b.1));
                                        for (code, disp) in &sorted {
                                            self.push_output(&format!("{code:<30}  {disp}"));
                                        }
                                        return;
                                    }
                                }
                            }
                            self.push_output(&format!("No group matching '{query}'"));
                        } else {
                            self.push_output(&format!("Scanners -- {total} total"));
                            self.push_output(&format!(
                                "{:<20}  {:<30}  {:>5}",
                                "Instrument", "Category", "Count"
                            ));
                            self.push_output(&"-".repeat(60));
                            let mut instruments: Vec<_> = tree.keys().collect();
                            instruments.sort();
                            for inst in instruments {
                                let cats = &tree[inst];
                                let mut cat_names: Vec<_> = cats.keys().collect();
                                cat_names.sort();
                                let mut first = true;
                                for cat in cat_names {
                                    let count = cats[cat].len();
                                    let inst_col = if first { inst.as_str() } else { "" };
                                    self.push_output(&format!(
                                        "{inst_col:<20}  {cat:<30}  {count:>5}"
                                    ));
                                    first = false;
                                }
                            }
                            self.push_output("\nUse 'list <group>' to expand a category.");
                        }
                    }
                    None => {
                        self.push_output("Could not connect to TWS");
                    }
                }
            }
            EngineEvent::PollCycleComplete {
                total_stocks,
                new_symbols,
                ..
            } => {
                let now = chrono::Local::now().format("%H:%M:%S");
                if new_symbols.is_empty() {
                    self.alert_line = format!(
                        "[{now}] Polling -- {} stocks, no new alerts (seen {})",
                        total_stocks,
                        self.engine.alert_seen.len()
                    );
                } else {
                    // Set alert line to top alert
                    if let Some(top) = self.engine.alert_rows.first() {
                        let chg = top.change_pct.unwrap_or(0.0);
                        let rvol = top.rvol.unwrap_or(0.0);
                        let cat = top.catalyst.as_deref().unwrap_or("");
                        let cat_short = if cat.len() > 30 {
                            format!("{}..", &cat[..28])
                        } else {
                            cat.to_string()
                        };
                        self.alert_line = format!(
                            "[{now}] ALERT: {} +{chg:.1}% RVol {rvol:.1}x ({} scanners) -- {cat_short} -- {} new stocks",
                            top.symbol, top.scanner_hits, new_symbols.len()
                        );
                    }
                }
            }
            EngineEvent::EnrichComplete { .. } => {
                // Engine already updated alert_rows — nothing to do
            }
            EngineEvent::PortDiscovered { .. } => {
                self.update_title();
            }
        }
    }
}

/// Run the TUI application. Creates its own tokio runtime for async ops.
pub fn run_tui() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let handle = rt.handle().clone();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create enrich channel first, then engine, then spawn worker with engine's bg_tx
    let (enrich_tx, enrich_rx) = std::sync::mpsc::channel::<crate::engine::EnrichRequest>();

    // Try to connect to Supabase
    crate::config::load_env();
    let db = if let Ok(config) = SupabaseConfig::from_env() {
        info!("Connected to Supabase");
        Some(SupabaseClient::new(config))
    } else {
        None
    };

    let mut app = App::new(AlertEngine::new(enrich_tx, Settings::default(), db));

    // Spawn enrichment worker with Supabase cache support
    let _worker = crate::engine::spawn_enrichment_worker(
        app.engine.bg_tx.clone(),
        enrich_rx,
        handle.clone(),
        app.engine.db.clone(),
    );

    // Probe TWS port
    app.engine.probe_port();
    app.update_title();

    // Initialize alerts from today's sightings
    app.engine.init_from_sightings(&handle);

    // Main event loop
    let mut poll_timer = std::time::Instant::now();

    loop {
        // Draw UI
        terminal.draw(|f| ui::draw(f, &app))?;

        // Handle events with timeout
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    KeyCode::Esc => {
                        app.mode = Mode::Alert;
                        app.update_title();
                    }
                    KeyCode::Insert => {
                        app.mode = Mode::Scan;
                        app.update_title();
                    }
                    KeyCode::Enter if app.mode == Mode::Scan => {
                        let input = app.input.clone();
                        app.input.clear();
                        app.input_cursor = 0;
                        app.handle_input(&input, &handle);
                    }
                    KeyCode::Char(c) if app.mode == Mode::Scan => {
                        app.input.insert(app.input_cursor, c);
                        app.input_cursor += 1;
                    }
                    KeyCode::Backspace if app.mode == Mode::Scan => {
                        if app.input_cursor > 0 {
                            app.input_cursor -= 1;
                            app.input.remove(app.input_cursor);
                        }
                    }
                    KeyCode::Left if app.mode == Mode::Scan => {
                        if app.input_cursor > 0 {
                            app.input_cursor -= 1;
                        }
                    }
                    KeyCode::Right if app.mode == Mode::Scan => {
                        if app.input_cursor < app.input.len() {
                            app.input_cursor += 1;
                        }
                    }
                    KeyCode::Up if app.mode == Mode::Scan => {
                        if !app.command_history.is_empty() {
                            if app.history_idx == -1 {
                                app.history_idx = app.command_history.len() as i32 - 1;
                            } else if app.history_idx > 0 {
                                app.history_idx -= 1;
                            }
                            if app.history_idx >= 0 {
                                app.input =
                                    app.command_history[app.history_idx as usize].clone();
                                app.input_cursor = app.input.len();
                            }
                        }
                    }
                    KeyCode::Down if app.mode == Mode::Scan => {
                        if app.history_idx >= 0 {
                            if (app.history_idx as usize) < app.command_history.len() - 1 {
                                app.history_idx += 1;
                                app.input =
                                    app.command_history[app.history_idx as usize].clone();
                            } else {
                                app.history_idx = -1;
                                app.input.clear();
                            }
                            app.input_cursor = app.input.len();
                        }
                    }
                    KeyCode::Up if app.mode == Mode::Alert => {
                        if app.selected_alert_row > 0 {
                            app.selected_alert_row -= 1;
                        }
                    }
                    KeyCode::Down if app.mode == Mode::Alert => {
                        if app.selected_alert_row + 1 < app.engine.alert_rows.len() {
                            app.selected_alert_row += 1;
                        }
                    }
                    KeyCode::PageUp => {
                        app.scroll_offset = app.scroll_offset.saturating_sub(10);
                    }
                    KeyCode::PageDown => {
                        app.scroll_offset = app.scroll_offset.saturating_add(10);
                    }
                    _ => {}
                }
            }
        }

        // Drain engine events
        let events = app.engine.tick(&handle);
        for event in events {
            app.handle_engine_event(event);
        }

        // Check poll timer
        if app.engine.polling
            && !app.engine.bg_busy
            && poll_timer.elapsed() >= Duration::from_secs(60)
        {
            poll_timer = std::time::Instant::now();
            app.engine.run_poll_scanners();
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn new_app() -> App {
        let (tx, _rx) = mpsc::channel();
        App::new(AlertEngine::new(tx, Settings::default(), None))
    }

    fn app_with_rt() -> (App, tokio::runtime::Runtime) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (tx, _rx) = mpsc::channel();
        (
            App::new(AlertEngine::new(tx, Settings::default(), None)),
            rt,
        )
    }

    #[test]
    fn test_app_initial_state() {
        let app = new_app();
        assert_eq!(app.mode, Mode::Alert);
        assert!(app.engine.alert_rows.is_empty());
        assert!(app.engine.alert_seen.is_empty());
        assert!(app.output_lines.is_empty());
        assert!(!app.should_quit);
        assert!(!app.engine.polling);
        assert_eq!(app.engine.settings.host, "127.0.0.1");
        assert_eq!(app.engine.settings.rows, 25);
    }

    #[test]
    fn test_quit_command() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("quit", &handle);
        assert!(app.should_quit);
    }

    #[test]
    fn test_exit_command() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("exit", &handle);
        assert!(app.should_quit);
    }

    #[test]
    fn test_q_command() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("q", &handle);
        assert!(app.should_quit);
    }

    #[test]
    fn test_help_command() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("help", &handle);
        assert!(!app.output_lines.is_empty());
        assert!(app.output_lines.iter().any(|l| l.contains("scan")));
        assert!(app.output_lines.iter().any(|l| l.contains("poll")));
    }

    #[test]
    fn test_unknown_command() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("foobar", &handle);
        assert!(app
            .output_lines
            .iter()
            .any(|l| l.contains("Unknown command")));
    }

    #[test]
    fn test_set_host() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("set host 192.168.1.1", &handle);
        assert_eq!(app.engine.settings.host, "192.168.1.1");
    }

    #[test]
    fn test_set_port() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("set port 7497", &handle);
        assert_eq!(app.engine.settings.port, Some(7497));
    }

    #[test]
    fn test_set_rows() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("set rows 50", &handle);
        assert_eq!(app.engine.settings.rows, 50);
    }

    #[test]
    fn test_set_minprice() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("set minprice 2.5", &handle);
        assert_eq!(app.engine.settings.min_price, Some(2.5));
    }

    #[test]
    fn test_set_maxprice() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("set maxprice 15", &handle);
        assert_eq!(app.engine.settings.max_price, Some(15.0));
    }

    #[test]
    fn test_set_maxprice_none() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.engine.settings.max_price = Some(20.0);
        app.handle_input("set maxprice none", &handle);
        assert_eq!(app.engine.settings.max_price, None);
    }

    #[test]
    fn test_set_unknown_key() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("set foobar 123", &handle);
        assert!(app
            .output_lines
            .iter()
            .any(|l| l.contains("Unknown setting")));
    }

    #[test]
    fn test_show_command() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("show", &handle);
        assert!(app.output_lines.iter().any(|l| l.contains("port")));
        assert!(app.output_lines.iter().any(|l| l.contains("host")));
        assert!(app.output_lines.iter().any(|l| l.contains("rows")));
    }

    #[test]
    fn test_aliases_command() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("aliases", &handle);
        assert!(app.output_lines.iter().any(|l| l.contains("gain")));
        assert!(app
            .output_lines
            .iter()
            .any(|l| l.contains("TOP_PERC_GAIN")));
    }

    #[test]
    fn test_mode_show() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("mode", &handle);
        assert!(app.output_lines.iter().any(|l| l.contains("alert")));
    }

    #[test]
    fn test_mode_switch_scan() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("mode scan", &handle);
        assert_eq!(app.mode, Mode::Scan);
    }

    #[test]
    fn test_mode_switch_alert() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.mode = Mode::Scan;
        app.handle_input("mode alert", &handle);
        assert_eq!(app.mode, Mode::Alert);
    }

    #[test]
    fn test_poll_status_off() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("poll", &handle);
        assert!(app.output_lines.iter().any(|l| l.contains("off")));
    }

    #[test]
    fn test_poll_clear() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.engine.alert_seen.insert("AAPL".to_string());
        app.engine.alert_seen.insert("TSLA".to_string());
        app.handle_input("poll clear", &handle);
        assert!(app.engine.alert_seen.is_empty());
        assert!(app.engine.alert_rows.is_empty());
    }

    #[test]
    fn test_scan_no_args() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("scan", &handle);
        assert!(app.output_lines.iter().any(|l| l.contains("Usage")));
    }

    #[test]
    fn test_command_history_recorded() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("help", &handle);
        app.handle_input("show", &handle);
        assert_eq!(app.command_history, vec!["help", "show"]);
    }

    #[test]
    fn test_command_history_no_duplicates() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("help", &handle);
        app.handle_input("help", &handle);
        assert_eq!(app.command_history.len(), 1);
    }

    #[test]
    fn test_empty_input_ignored() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("", &handle);
        assert!(app.command_history.is_empty());
        assert!(app.output_lines.is_empty());
    }

    #[test]
    fn test_whitespace_input_ignored() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("   ", &handle);
        assert!(app.command_history.is_empty());
    }

    #[test]
    fn test_update_title_alert_mode() {
        let mut app = new_app();
        app.mode = Mode::Alert;
        app.update_title();
        assert!(app.title.contains("[ALERT]"));
    }

    #[test]
    fn test_update_title_scan_mode() {
        let mut app = new_app();
        app.mode = Mode::Scan;
        app.update_title();
        assert!(app.title.contains("[SCAN]"));
    }

    #[test]
    fn test_update_title_with_port() {
        let mut app = new_app();
        app.engine.connected_port = Some(7500);
        app.update_title();
        assert!(app.title.contains("7500"));
    }

    #[test]
    fn test_update_title_auto_port() {
        let mut app = new_app();
        app.update_title();
        assert!(app.title.contains("auto"));
    }

    #[test]
    fn test_history_no_db() {
        let (mut app, rt) = app_with_rt();
        let handle = rt.handle().clone();
        app.handle_input("history", &handle);
        assert!(app
            .output_lines
            .iter()
            .any(|l| l.contains("Supabase not connected")));
    }

    #[test]
    fn test_enrichment_data_news_headlines() {
        use crate::models::NewsHeadline;
        let data = crate::enrichment::EnrichmentData {
            news_headlines: vec![
                NewsHeadline { title: "Headline 1".to_string(), published: Some(1700000000) },
                NewsHeadline { title: "Headline 2".to_string(), published: None },
            ],
            ..Default::default()
        };
        assert_eq!(data.news_headlines.len(), 2);
        assert_eq!(data.news_headlines[0].title, "Headline 1");
        assert_eq!(data.news_headlines[0].published, Some(1700000000));
    }
}
