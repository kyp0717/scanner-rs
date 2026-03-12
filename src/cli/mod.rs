use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;

use crate::config::SupabaseConfig;
use crate::engine::{AlertEngine, EngineEvent};
use crate::enrichment;
use crate::history::{self, SupabaseClient};
use crate::models::*;
use crate::scanner;
use crate::tws;

/// Log a timestamped message. In text mode goes to stdout; in JSON mode goes to stderr.
fn log_alert(json: bool, msg: &str) {
    let ts = chrono::Local::now().format("%H:%M:%S");
    let line = format!("[{ts}] [LOG] {msg}");
    if json {
        eprintln!("{line}");
    } else {
        println!("{line}");
    }
}

/// One-shot scan: connect to TWS, run scanner, enrich, print results.
pub async fn cmd_scan(
    code: &str,
    host: &str,
    port: Option<u16>,
    rows: u32,
    min_price: f64,
    max_price: Option<f64>,
) -> Result<()> {
    let scanner_code = resolve_scanner(code);
    let ports: Vec<u16> = port
        .map(|p| vec![p])
        .unwrap_or_else(|| DEFAULT_PORTS.to_vec());

    if code.to_lowercase() == "list" {
        match tws::fetch_scanner_params(host, &ports, 3).await {
            Some(xml) => tws::print_scanner_params(&xml, None),
            None => eprintln!("Could not connect to TWS"),
        }
        return Ok(());
    }

    let (mut results, _port) =
        tws::run_scan(&scanner_code, host, &ports, 1, rows, Some(min_price), max_price).await;

    if !results.is_empty() {
        println!("Enriching with Yahoo Finance...");
        enrichment::enrich_results(&mut results).await;
    }

    scanner::print_results(&results);
    Ok(())
}

/// Fetch and print scanner parameters / groups.
pub async fn cmd_list(group: Option<&str>, host: &str, port: Option<u16>) -> Result<()> {
    let ports: Vec<u16> = port
        .map(|p| vec![p])
        .unwrap_or_else(|| DEFAULT_PORTS.to_vec());
    match tws::fetch_scanner_params(host, &ports, 3).await {
        Some(xml) => tws::print_scanner_params(&xml, group),
        None => eprintln!("Could not connect to TWS"),
    }
    Ok(())
}

/// Query and print Supabase tws_scans history.
pub async fn cmd_history(what: Option<&str>) -> Result<()> {
    let config = SupabaseConfig::from_env()?;
    let db = SupabaseClient::new(config);

    match what {
        Some("clear") => {
            let count = db.clear_history().await?;
            println!("Cleared {count} stocks from history");
        }
        Some("all") => {
            let stocks = db.get_history(500).await?;
            history::print_history(&stocks, "All history");
        }
        Some("today") | None => {
            let stocks = db.get_today().await?;
            history::print_history(&stocks, "Today");
        }
        Some(n) => {
            if let Ok(limit) = n.parse::<u32>() {
                let stocks = db.get_history(limit).await?;
                history::print_history(&stocks, &format!("Last {limit}"));
            } else {
                eprintln!("Usage: scanner history [today|all|clear|N]");
            }
        }
    }
    Ok(())
}

/// Enrich symbols with Yahoo Finance data and print results.
pub async fn cmd_enrich(symbols: &[String]) -> Result<()> {
    if symbols.is_empty() {
        eprintln!("Usage: scanner enrich AAPL TSLA ...");
        return Ok(());
    }

    let client = reqwest::Client::new();
    for sym in symbols {
        println!("Enriching {sym}...");
        let data = enrichment::fetch_enrichment(&client, sym).await;
        println!("  Name:        {}", data.name.as_deref().unwrap_or("-"));
        println!("  Sector:      {}", data.sector.as_deref().unwrap_or("-"));
        println!(
            "  Industry:    {}",
            data.industry.as_deref().unwrap_or("-")
        );
        println!(
            "  Float:       {}",
            data.float_shares
                .map(|f| format!("{:.1}M", f / 1e6))
                .unwrap_or("-".into())
        );
        println!(
            "  Short%:      {}",
            data.short_pct
                .map(|p| format!("{:.1}%", p * 100.0))
                .unwrap_or("-".into())
        );
        println!(
            "  Avg Vol 10d: {}",
            data.avg_volume_10d
                .map(|v| format!("{v}"))
                .unwrap_or("-".into())
        );
        println!(
            "  Avg Vol 3mo: {}",
            data.avg_volume
                .map(|v| format!("{v}"))
                .unwrap_or("-".into())
        );
        println!(
            "  Catalyst:    {}",
            data.catalyst.as_deref().unwrap_or("none")
        );
        if let Some(ct) = data.catalyst_time {
            println!("               {}", format_time_ago(ct));
        }
        if !data.news_headlines.is_empty() {
            println!("  Headlines:");
            for h in data.news_headlines.iter().take(5) {
                let ago = h.published.map(format_time_ago).unwrap_or_default();
                if ago.is_empty() {
                    println!("    > {}", h.title);
                } else {
                    println!("    > {} — \"{}\"", ago, h.title);
                }
            }
        }
        println!();
    }
    Ok(())
}

/// Cross-check volume: fetch 5-min bars from IB historical data, sum volumes,
/// and compare with the snapshot tick Volume value.
pub async fn cmd_volume(symbols: &[String], host: &str, port: Option<u16>) -> Result<()> {
    if symbols.is_empty() {
        eprintln!("Usage: scanner volume LCUT AAPL ...");
        return Ok(());
    }

    let ports: Vec<u16> = port
        .map(|p| vec![p])
        .unwrap_or_else(|| DEFAULT_PORTS.to_vec());

    for sym in symbols {
        println!("=== {sym} ===");
        match tws::fetch_volume_check(sym, host, &ports).await {
            Ok((bar_sum, tick_vol, bars)) => {
                println!(
                    "  5-min bar volume sum : {:.0}  ({})",
                    bar_sum,
                    format_vol(bar_sum as i64)
                );
                match tick_vol {
                    Some(v) => println!(
                        "  Tick Volume (×100)   : {}  ({})",
                        v,
                        format_vol(v)
                    ),
                    None => println!("  Tick Volume (×100)   : (none)"),
                }
                if let Some(v) = tick_vol {
                    if bar_sum > 0.0 {
                        let ratio = v as f64 / bar_sum;
                        println!("  Ratio (tick / bars)  : {ratio:.4}x");
                    }
                }
                println!("  Bars: {}", bars.len());
                // Show last 5 bars
                let show = if bars.len() > 5 {
                    &bars[bars.len() - 5..]
                } else {
                    &bars
                };
                for (ts, close, vol) in show {
                    println!("    {ts}  close={close:.2}  vol={vol:.0}");
                }
            }
            Err(e) => {
                eprintln!("  Error: {e}");
            }
        }
        println!();
    }
    Ok(())
}

fn format_vol(v: i64) -> String {
    let shares = v as f64 * 100.0;
    if shares >= 1_000_000.0 {
        format!("{:.1}M", shares / 1_000_000.0)
    } else if shares >= 1_000.0 {
        format!("{:.1}K", shares / 1_000.0)
    } else {
        format!("{:.0}", shares)
    }
}

/// Format a Unix epoch timestamp as a relative "time ago" string.
fn format_time_ago(epoch: i64) -> String {
    let now = chrono::Utc::now().timestamp();
    let diff = now - epoch;
    if diff < 0 {
        return "just now".to_string();
    }
    let mins = diff / 60;
    let hours = diff / 3600;
    let days = diff / 86400;
    if mins < 1 {
        "just now".to_string()
    } else if mins < 60 {
        format!("{mins}m ago")
    } else if hours < 24 {
        format!("{hours}h ago")
    } else {
        format!("{days}d ago")
    }
}

/// Print configuration.
pub fn cmd_config() {
    println!("Configuration:");
    println!(
        "  SUPABASE_URL = {}",
        std::env::var("SUPABASE_URL").unwrap_or_else(|_| "(not set)".into())
    );
    println!(
        "  SUPABASE_ANON_KEY = {}",
        if std::env::var("SUPABASE_ANON_KEY").is_ok() {
            "(set)"
        } else {
            "(not set)"
        }
    );
    println!("  Default ports: {:?}", DEFAULT_PORTS);
}

/// Headless alert streamer — polls TWS scanners and prints alerts to stdout.
pub fn run_alert(host: &str, port: Option<u16>, json: bool) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let handle = rt.handle().clone();

    // Setup Supabase
    crate::config::load_env();
    let db = if let Ok(config) = SupabaseConfig::from_env() {
        Some(SupabaseClient::new(config))
    } else {
        None
    };

    // Create enrich channel, then engine, then spawn worker
    let (enrich_tx, enrich_rx) = mpsc::channel();

    let mut settings = Settings::default();
    settings.host = host.to_string();
    settings.port = port;

    let mut engine = AlertEngine::new(enrich_tx, settings, db);

    // Spawn enrichment worker with Supabase cache support
    let _worker = crate::engine::spawn_enrichment_worker(
        engine.bg_tx.clone(),
        enrich_rx,
        handle.clone(),
        engine.db.clone(),
    );

    // Spawn market data streaming worker
    let (mktdata_tx, mktdata_rx) = mpsc::channel::<crate::engine::MktDataRequest>();
    let mktdata_host = engine.settings.host.clone();
    let mktdata_ports: Vec<u16> = engine.settings.port
        .map(|p| vec![p])
        .unwrap_or_else(|| DEFAULT_PORTS.to_vec());
    let _mktdata_worker = crate::engine::spawn_market_data_worker(
        engine.bg_tx.clone(),
        mktdata_rx,
        mktdata_host,
        mktdata_ports,
    );
    engine.mktdata_tx = Some(mktdata_tx);

    let ports_desc = engine.settings.port
        .map(|p| format!("{p}"))
        .unwrap_or_else(|| format!("{:?}", DEFAULT_PORTS));
    log_alert(json, &format!("Probing TWS on ports {ports_desc}..."));

    // Probe TWS port
    engine.probe_port();
    if let Some(p) = engine.connected_port {
        log_alert(json, &format!("TWS connected on port {p}"));
    } else {
        log_alert(json, "TWS unavailable, alerts will be empty");
    }

    // Initialize from tws_scans
    log_alert(json, "Loading today's tws_scans from Supabase...");
    let (loaded, needs_enrich) = engine.init_from_tws_scans(&handle);
    log_alert(json, &format!("Loaded {loaded} stocks from tws_scans, {needs_enrich} queued for enrichment"));

    // Subscribe existing alert rows to streaming market data
    let existing_syms: Vec<String> = engine.alert_rows.iter().map(|r| r.symbol.clone()).collect();
    for sym in &existing_syms {
        engine.subscribe_market_data(sym, "USD");
    }

    // Start polling
    engine.poll_on();
    log_alert(json, "Starting poll (8 scanners, 15s cycle). Ctrl+C to stop.");

    // Setup Ctrl+C handler
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    ctrlc_flag(&r);

    let mut poll_timer = std::time::Instant::now();

    while running.load(std::sync::atomic::Ordering::Relaxed) {
        let events = engine.tick(&handle);
        for event in events {
            match event {
                EngineEvent::PollCycleComplete {
                    total_stocks,
                    new_symbols,
                    scanners_run,
                    elapsed_secs,
                } => {
                    log_alert(json, &format!(
                        "Poll cycle complete: {scanners_run} scanners, {total_stocks} stocks, {} new alerts in {elapsed_secs:.1}s (total seen: {})",
                        new_symbols.len(),
                        engine.alert_seen.len()
                    ));
                    for sym in &new_symbols {
                        if let Some(row) =
                            engine.alert_rows.iter().find(|r| r.symbol == *sym)
                        {
                            if json {
                                println!(
                                    "{}",
                                    serde_json::to_string(row).unwrap_or_default()
                                );
                            } else {
                                let chg = row
                                    .change_pct
                                    .map(|c| format!("{c:+.1}%"))
                                    .unwrap_or("-".into());
                                let price = row
                                    .last
                                    .map(|p| format!("{p:.2}"))
                                    .unwrap_or("-".into());
                                println!(
                                    "[{}] [ALERT] {:<6}  ${:>7}  {:>8}  {}/8 scanners",
                                    row.alert_time,
                                    row.symbol,
                                    price,
                                    chg,
                                    row.scanner_hits,
                                );
                            }
                        }
                    }
                }
                EngineEvent::EnrichComplete { ref symbol, .. } => {
                    if let Some(row) =
                        engine.alert_rows.iter().find(|r| r.symbol == *symbol)
                    {
                        if json {
                            println!(
                                "{}",
                                serde_json::to_string(row).unwrap_or_default()
                            );
                        } else {
                            // Re-display alert line
                            let chg = row
                                .change_pct
                                .map(|c| format!("{c:+.1}%"))
                                .unwrap_or("-".into());
                            let price = row
                                .last
                                .map(|p| format!("{p:.2}"))
                                .unwrap_or("-".into());
                            println!(
                                "[{}] [ALERT] {:<6}  ${:>7}  {:>8}  {}/8 scanners",
                                row.alert_time,
                                row.symbol,
                                price,
                                chg,
                                row.scanner_hits,
                            );

                            // Fundamentals card
                            let name = row.name.as_deref().unwrap_or("-");
                            let sector = row.sector.as_deref().unwrap_or("-");
                            let float = row
                                .float_shares
                                .map(|f| {
                                    if f >= 1e9 {
                                        format!("{:.1}B", f / 1e9)
                                    } else if f >= 1e6 {
                                        format!("{:.1}M", f / 1e6)
                                    } else {
                                        format!("{:.0}", f)
                                    }
                                })
                                .unwrap_or("-".into());
                            let short = row
                                .short_pct
                                .map(|p| format!("{:.1}%", p * 100.0))
                                .unwrap_or("-".into());
                            let rvol = row
                                .rvol
                                .map(|r| format!("{r:.1}x"))
                                .unwrap_or("-".into());

                            let ts = chrono::Local::now().format("%H:%M:%S");
                            println!(
                                "[{ts}] [FUNDAMENTALS] {}  {}  ({})",
                                row.symbol, name, sector
                            );
                            println!(
                                "           Float: {}  |  Short: {}  |  RVol: {}",
                                float, short, rvol
                            );

                            // Catalyst with time
                            if let Some(ref cat) = row.catalyst {
                                let cat_ago = row.catalyst_time
                                    .map(|t| format!("{} — ", format_time_ago(t)))
                                    .unwrap_or_default();
                                println!(
                                    "           Catalyst: {cat_ago}\"{cat}\""
                                );
                            }

                            // Headlines
                            if !row.news_headlines.is_empty() {
                                println!("           Headlines:");
                                for h in row.news_headlines.iter().take(5) {
                                    let ago = h.published
                                        .map(|t| format!("{} — ", format_time_ago(t)))
                                        .unwrap_or_default();
                                    println!(
                                        "             > {ago}\"{}\"",
                                        h.title
                                    );
                                }
                            }
                        }
                    }
                }
                EngineEvent::PortDiscovered { port } => {
                    log_alert(json, &format!("TWS port discovered: {port}"));
                }
                _ => {}
            }
        }

        // Check poll timer
        if engine.polling
            && !engine.poll_busy
            && poll_timer.elapsed() >= Duration::from_secs(15)
        {
            poll_timer = std::time::Instant::now();
            log_alert(json, "Starting poll cycle...");
            engine.run_poll_scanners();
        }

        std::thread::sleep(Duration::from_millis(100));
    }

    let alert_count = engine.alert_rows.len();
    log_alert(json, &format!("Shutting down (seen {} stocks, {} alerts)", engine.alert_seen.len(), alert_count));
    Ok(())
}

/// Set an atomic flag to false on Ctrl+C.
fn ctrlc_flag(flag: &std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let f = flag.clone();
    let _ = ctrlc::set_handler(move || {
        f.store(false, std::sync::atomic::Ordering::Relaxed);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmd_config_no_panic() {
        // Just ensure it doesn't panic
        cmd_config();
    }
}
