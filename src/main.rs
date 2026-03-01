use anyhow::Result;
use clap::{Parser, Subcommand};

use scanner_rs::cli;
use scanner_rs::config;
use scanner_rs::tui;

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
    /// Stream momentum alerts to stdout (headless mode)
    Alert {
        /// TWS host
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// TWS port (auto-detects 7500/7497 if omitted)
        #[arg(long)]
        port: Option<u16>,
        /// Output alerts as JSON lines
        #[arg(long)]
        json: bool,
    },
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

    let cli_args = Cli::parse();

    match cli_args.command {
        // TUI mode: runs its own tokio runtime internally
        Some(Commands::Tui) | None => {
            tui::run_tui()?;
        }

        // Alert mode: runs its own tokio runtime internally
        Some(Commands::Alert { host, port, json }) => {
            cli::run_alert(&host, port, json)?;
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
            cli::cmd_scan(&code, &host, port, rows, min_price, max_price).await?;
        }

        Commands::List { group, host, port } => {
            cli::cmd_list(group.as_deref(), &host, port).await?;
        }

        Commands::History { what } => {
            cli::cmd_history(what.as_deref()).await?;
        }

        Commands::Enrich { symbols } => {
            cli::cmd_enrich(&symbols).await?;
        }

        Commands::Config { what: _ } => {
            cli::cmd_config();
        }

        Commands::Tui | Commands::Alert { .. } => unreachable!(),
    }

    Ok(())
}
