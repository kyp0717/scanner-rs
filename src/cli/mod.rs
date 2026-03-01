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
        match tws::fetch_scanner_params(host, &ports, 3) {
            Some(xml) => tws::print_scanner_params(&xml, None),
            None => eprintln!("Could not connect to TWS"),
        }
        return Ok(());
    }

    let (mut results, _port) =
        tws::run_scan(&scanner_code, host, &ports, 1, rows, Some(min_price), max_price);

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
    match tws::fetch_scanner_params(host, &ports, 3) {
        Some(xml) => tws::print_scanner_params(&xml, group),
        None => eprintln!("Could not connect to TWS"),
    }
    Ok(())
}

/// Query and print Supabase sightings history.
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
            "  Avg Volume:  {}",
            data.avg_volume
                .map(|v| format!("{v}"))
                .unwrap_or("-".into())
        );
        println!(
            "  Catalyst:    {}",
            data.catalyst.as_deref().unwrap_or("none")
        );
        println!();
    }
    Ok(())
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

    // Spawn enrichment worker with engine's bg_tx
    {
        let bg_tx = engine.bg_tx.clone();
        let rt_handle = handle.clone();
        std::thread::spawn(move || {
            let client = reqwest::Client::new();
            let mut heap =
                std::collections::BinaryHeap::<crate::engine::EnrichRequest>::new();
            let mut enriched_set = std::collections::HashSet::<String>::new();

            loop {
                loop {
                    match enrich_rx.try_recv() {
                        Ok(req) => {
                            if req.symbol.is_empty() {
                                enriched_set.clear();
                                heap.clear();
                                continue;
                            }
                            if !enriched_set.contains(&req.symbol) {
                                heap.push(req);
                            }
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return,
                    }
                }

                if let Some(req) = heap.pop() {
                    if enriched_set.contains(&req.symbol) {
                        continue;
                    }
                    enriched_set.insert(req.symbol.clone());
                    let data = rt_handle
                        .block_on(crate::enrichment::fetch_enrichment(&client, &req.symbol));
                    let _ = bg_tx.send(crate::engine::BgMessage::EnrichComplete {
                        symbol: req.symbol,
                        data,
                    });
                } else {
                    match enrich_rx.recv_timeout(Duration::from_secs(1)) {
                        Ok(req) => {
                            if req.symbol.is_empty() {
                                enriched_set.clear();
                            } else if !enriched_set.contains(&req.symbol) {
                                heap.push(req);
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }
            }
        });
    }

    eprintln!("Scanner alert mode starting...");

    // Probe TWS port
    engine.probe_port();
    if let Some(p) = engine.connected_port {
        eprintln!("Connected to TWS on port {p}");
    } else {
        eprintln!("Warning: Could not connect to TWS");
    }

    // Initialize from sightings
    engine.init_from_sightings(&handle);

    // Start polling
    engine.poll_on();
    eprintln!("Polling started (60s interval). Press Ctrl+C to stop.");

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
                } => {
                    if !new_symbols.is_empty() {
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
                                        "[{}] ALERT  {:<6}  {:>8}  {:>8}  {}/8 scanners",
                                        row.alert_time,
                                        row.symbol,
                                        price,
                                        chg,
                                        row.scanner_hits,
                                    );
                                }
                            }
                        }
                    } else {
                        let now = chrono::Local::now().format("%H:%M:%S");
                        eprintln!(
                            "[{now}] Poll complete -- {total_stocks} stocks, no new alerts (seen {})",
                            engine.alert_seen.len()
                        );
                    }
                }
                EngineEvent::EnrichComplete { ref symbol } => {
                    if json {
                        if let Some(row) =
                            engine.alert_rows.iter().find(|r| r.symbol == *symbol)
                        {
                            println!(
                                "{}",
                                serde_json::to_string(row).unwrap_or_default()
                            );
                        }
                    } else {
                        if let Some(row) =
                            engine.alert_rows.iter().find(|r| r.symbol == *symbol)
                        {
                            let cat = row.catalyst.as_deref().unwrap_or("-");
                            let name = row.name.as_deref().unwrap_or("-");
                            let rvol = row
                                .rvol
                                .map(|r| format!("{r:.1}x"))
                                .unwrap_or("-".into());
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
                            eprintln!(
                                "  ENRICH {:<6}  RVol={:<6}  Float={:<8}  Cat={}  Name={}",
                                symbol, rvol, float, cat, name
                            );
                        }
                    }
                }
                EngineEvent::PortDiscovered { port } => {
                    eprintln!("TWS port discovered: {port}");
                }
                _ => {}
            }
        }

        // Check poll timer
        if engine.polling
            && !engine.bg_busy
            && poll_timer.elapsed() >= Duration::from_secs(60)
        {
            poll_timer = std::time::Instant::now();
            engine.run_poll_scanners();
        }

        std::thread::sleep(Duration::from_millis(100));
    }

    eprintln!("\nShutting down...");
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
