use anyhow::Result;
use clap::{Parser, Subcommand};

use scanner_rs::config::{self, SupabaseConfig};
use scanner_rs::enrichment;
use scanner_rs::history::{self, SupabaseClient};
use scanner_rs::models::{self, DEFAULT_PORTS};
use scanner_rs::scanner;
use scanner_rs::tui;
use scanner_rs::tws;

#[derive(Parser)]
#[command(name = "scanner", about = "TWS Momentum Stock Scanner")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a TWS scanner subscription
    Scan {
        /// Scanner code or alias (e.g., TOP_PERC_GAIN, gain, hot)
        code: String,
        /// TWS host
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// TWS port (auto-detects 7500/7497 if omitted)
        #[arg(long)]
        port: Option<u16>,
        /// Number of scanner rows
        #[arg(long, default_value = "25")]
        rows: u32,
        /// Minimum price filter
        #[arg(long, default_value = "1")]
        min_price: f64,
        /// Maximum price filter
        #[arg(long)]
        max_price: Option<f64>,
        /// List scanner parameters instead of running a scan
        #[arg(long)]
        list: bool,
    },
    /// List available scanners from TWS
    List {
        /// Group to expand (fuzzy match), or omit for summary
        group: Option<String>,
        /// TWS host
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// TWS port
        #[arg(long)]
        port: Option<u16>,
    },
    /// Query Supabase sightings history
    History {
        /// Subcommand: today (default), all, clear, or a number
        what: Option<String>,
    },
    /// Enrich symbols with Yahoo Finance data (for testing)
    Enrich {
        /// Symbols to enrich
        symbols: Vec<String>,
    },
    /// Show current configuration
    Config {
        /// Subcommand: show
        what: Option<String>,
    },
    /// Launch the interactive TUI
    Tui,
}

fn init_logging() -> tracing_appender::non_blocking::WorkerGuard {
    let var_dir = std::path::Path::new("var");
    if !var_dir.exists() {
        let _ = std::fs::create_dir_all(var_dir);
    }
    let file_appender = tracing_appender::rolling::daily("var", "scanner.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
    guard
}

fn main() -> Result<()> {
    let _guard = init_logging();
    config::load_env();

    let cli = Cli::parse();

    match cli.command {
        // TUI mode: runs its own tokio runtime internally
        Some(Commands::Tui) | None => {
            tui::run_tui()?;
        }

        // All other commands use a tokio runtime
        other => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(run_command(other.unwrap()))?;
        }
    }

    Ok(())
}

async fn run_command(cmd: Commands) -> Result<()> {
    match cmd {
        Commands::Scan {
            code,
            host,
            port,
            rows,
            min_price,
            max_price,
            list: _,
        } => {
            let scanner_code = models::resolve_scanner(&code);
            let ports: Vec<u16> = port.map(|p| vec![p]).unwrap_or_else(|| DEFAULT_PORTS.to_vec());

            if code.to_lowercase() == "list" {
                match tws::fetch_scanner_params(&host, &ports, 3) {
                    Some(xml) => tws::print_scanner_params(&xml, None),
                    None => eprintln!("Could not connect to TWS"),
                }
                return Ok(());
            }

            let mut results =
                tws::run_scan(&scanner_code, &host, &ports, 1, rows, Some(min_price), max_price);

            if !results.is_empty() {
                println!("Enriching with Yahoo Finance...");
                enrichment::enrich_results(&mut results).await;
            }

            scanner::print_results(&results);
        }

        Commands::List { group, host, port } => {
            let ports: Vec<u16> = port.map(|p| vec![p]).unwrap_or_else(|| DEFAULT_PORTS.to_vec());
            match tws::fetch_scanner_params(&host, &ports, 3) {
                Some(xml) => tws::print_scanner_params(&xml, group.as_deref()),
                None => eprintln!("Could not connect to TWS"),
            }
        }

        Commands::History { what } => {
            let config = SupabaseConfig::from_env()?;
            let db = SupabaseClient::new(config);

            match what.as_deref() {
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
        }

        Commands::Enrich { symbols } => {
            if symbols.is_empty() {
                eprintln!("Usage: scanner enrich AAPL TSLA ...");
                return Ok(());
            }

            let client = reqwest::Client::new();
            for sym in &symbols {
                println!("Enriching {sym}...");
                let data = enrichment::fetch_enrichment(&client, sym).await;
                println!("  Name:        {}", data.name.as_deref().unwrap_or("-"));
                println!("  Sector:      {}", data.sector.as_deref().unwrap_or("-"));
                println!("  Industry:    {}", data.industry.as_deref().unwrap_or("-"));
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
        }

        Commands::Config { what: _ } => {
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

        Commands::Tui => unreachable!(),
    }

    Ok(())
}
