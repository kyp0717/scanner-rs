# Scanner-Rs Project

## Goal
A CLI momentum stock alert system for premarket and regular trading hours (RTH).
Uses TWS scanner combinations + Yahoo Finance news confirmation to alert traders
when stocks are moving with a catalyst. Rust port of scanner-py.

## Momentum Definition (Ross Cameron / Warrior Trading)
1. **Relative Volume >= 5x** — current volume vs 30-day average
2. **Gap/Change >= 10%** — price up at least 10% from previous close
3. **News Catalyst** — confirmed via Yahoo Finance (earnings, FDA, PR, contracts, etc.)
4. **Price $1-$20** — small-cap sweet spot for explosive moves
5. **Float < 10M shares** — low float = supply/demand imbalance

### Scanner Strategy
- **Premarket**: Gapper scanner — stocks gapping up on volume before open
- **RTH**: Momentum scanner — stocks making new highs with volume surge
- Combine multiple TWS scanners and cross-reference to reduce false positives
- Confirm moves against Yahoo Finance for news catalyst before alerting

## Repository
Private repo — `.env` and `.mcp.json` are tracked in git intentionally.

## Build & Run
```bash
cargo build                    # debug build
cargo build --release          # release build
cargo run                      # launch GUI app
cargo run -- --help            # show CLI help
cargo test                     # run all tests
```

## CLI Testing Commands
The CLI has subcommands to test individual components without the full GUI.
**The CLI (`src/cli/`) is essential for testing and must never be removed.**

```bash
# Test scanner connection and scanning
cargo run -- scan TOP_PERC_GAIN
cargo run -- scan gain --rows 10
cargo run -- scan list

# Test Supabase connection and history
cargo run -- history
cargo run -- history today
cargo run -- history clear

# Test Yahoo Finance enrichment
cargo run -- enrich AAPL TSLA NVDA

# Test configuration
cargo run -- config show

# Headless alert mode (streams alerts to stdout)
cargo run -- alert
cargo run -- alert --port 7497
cargo run -- alert --json          # JSON lines on stdout, logs on stderr

# Launch full GUI
cargo run -- gui
```

## Project Structure (Modular)
```
src/
  main.rs          — CLI entry point with clap subcommands
  lib.rs           — re-exports all modules
  config.rs        — settings, .env loading, constants
  models.rs        — shared data types (ScanResult, AlertRow, Settings, etc.)
  tws/
    mod.rs         — TWS scanner via ibapi crate + XML parsing
  engine/
    mod.rs         — core alert engine (shared by GUI and CLI)
  cli/
    mod.rs         — CLI subcommand handlers (scan, alert, history, enrich)
  scanner.rs       — scanner logic, enrichment, filtering, table display
  history.rs       — Supabase persistence (tws_scans CRUD)
  enrichment.rs    — Yahoo Finance data fetching
  catalyst.rs      — news catalyst classification
  gui/
    mod.rs         — iced GUI module
    app.rs         — app state, Message enum, iced integration, tests
    theme.rs       — dark theme colors and widget styles
    components/
      mod.rs       — shared components
      side_rail.rs — 48px icon rail with SVG icons + tooltips
    views/
      mod.rs       — view module
      monitor.rs   — alerts view: alert table + detail panel (default view)
      scanner.rs   — command input + scan output
      log.rs       — combined log view (engine events + tws_scans, tagged by source)
      settings.rs  — settings + connection status
  error.rs         — error types
```

## Architecture: CLI ↔ Engine ↔ GUI

```
main.rs (clap router)
  ├── scan/list/enrich/history  →  direct tws/enrichment calls (no engine)
  ├── alert                     →  AlertEngine + stdout loop
  └── gui (default)             →  AlertEngine + iced event loop
```

**AlertEngine** (`engine/mod.rs`) is the shared core. It owns scanning, polling,
enrichment queueing, alert tracking, and Supabase persistence. Both CLI alert
mode and GUI create an engine and consume the same `EngineEvent` variants:
- `ScanComplete` — one-shot scan finished
- `PollCycleComplete` — all 8 scanners polled, new symbols detected
- `EnrichComplete` — Yahoo Finance data arrived for a symbol
- `PortDiscovered` — TWS port auto-detected

Background work (TWS scanning, Yahoo enrichment) runs in OS threads via `mpsc`
channels. Each thread creates its own `tokio::runtime::Runtime` for async ibapi
calls. The main thread (CLI loop or GUI event loop) calls `engine.tick()` each
iteration to drain events without blocking.

## Dependencies
- `ibapi` — IB TWS API client (scanner subscriptions, connection handling)
- `tokio` — async runtime
- `reqwest` — HTTP client (Yahoo Finance, Supabase REST)
- `serde` / `serde_json` — serialization
- `clap` — CLI argument parsing
- `iced` — GUI framework (with svg feature)
- `dotenv` — .env loading
- `chrono` — timestamps
- `tracing` — structured logging
- `quick-xml` — XML parsing (scanner params)
- `anyhow` — error handling

## Alert Scanners
| # | Scanner Code | Client ID | Description |
|---|-------------|-----------|-------------|
| 1 | `HOT_BY_VOLUME` | 10 | Unusual volume surge vs recent average |
| 2 | `TOP_PERC_GAIN` | 11 | Biggest percentage gainers |
| 3 | `MOST_ACTIVE` | 12 | Highest absolute volume |
| 4 | `HIGH_OPEN_GAP` | 13 | Gapping up vs previous close |
| 5 | `TOP_TRADE_COUNT` | 14 | Most trades (retail interest) |
| 6 | `HOT_BY_PRICE` | 15 | Rapid price movement |
| 7 | `TOP_VOLUME_RATE` | 16 | Volume acceleration |
| 8 | `HIGH_VS_52W_HL` | 17 | New 52-week highs |

## GUI Scanner Commands
```
scan <alias|code> [--rows N] [--min-price N] [--max-price N]
list                    3-level picker: instrument -> category -> scanner
list <group>            Fuzzy expand category
poll                    Show polling status
poll on|off             Start/stop background momentum polling (60s)
poll clear              Clear seen-set (re-alert)
history                 Show today's tracked stocks
history all             Show all historical stocks
history clear           Clear entire history
set <key> <value>       Change setting (port, host, rows, minprice, maxprice)
show                    Current settings
aliases                 Alias map
help                    Help text
quit / exit / q         Exit
```

## Scanner Aliases
- `gain` -> `TOP_PERC_GAIN`
- `hot` -> `HOT_BY_VOLUME`
- `active` -> `MOST_ACTIVE`
- `lose` -> `TOP_PERC_LOSE`
- `gap` -> `HIGH_OPEN_GAP`
- `gapdown` -> `LOW_OPEN_GAP`

## Supabase Persistence
Table `tws_scans` in hosted Postgres. Credentials in `.env`.
Columns: symbol (unique), first/last seen timestamps, scanners (comma-sep),
hit_count, last_price, change_pct, rvol, float_shares, catalyst, catalyst_time,
name, sector, industry, short_pct, avg_volume, news_headlines, enriched_at.

## Design Rules
- **Modular code** — each module handles one concern, testable in isolation
- **CLI-first testing** — ALWAYS verify data flow via CLI before integrating into GUI
- **Dark theme** — dark background GUI aesthetic
- **Keep it simple** — iced for layout, no over-engineering
- **Tests first** — unit tests for all pure logic (filtering, classification, enrichment)

## Testing Approach — CLI First (MANDATORY)
Before integrating any data pipeline change into the GUI, **you MUST verify it
works via CLI first**. The CLI subcommands exist specifically for this purpose.

Testing checklist for data flow changes:
1. `cargo test` — all unit tests pass
2. `cargo run -- enrich AAPL TSLA` — verify Yahoo Finance enrichment returns data
3. `cargo run -- scan gain --rows 5` — verify scanner returns results with prices
4. `cargo run -- history` — verify Supabase reads work
5. `cargo run -- alert` (briefly) — verify poll cycle produces events

Only after CLI confirms data flows correctly should changes be wired into the GUI.
If CLI shows "-" or empty data, fix the data layer first — don't debug in the GUI.

## Logging
- **tracing** writes structured logs to `var/scanner.log` (rolling daily)
- **Alert CLI** uses `log_alert()` helper: `[HH:MM:SS] [LOG] message`
  - Text mode (`--json` off): logs go to stdout interleaved with alerts
  - JSON mode (`--json` on): logs go to stderr, stdout is clean JSON lines
- Engine-level logging uses `tracing::{info, warn}` — not `eprintln!`

## TWS Connection
Uses `ibapi` crate for all TWS communication (connection, scanner subscriptions,
scanner parameters). No hand-rolled protocol code.
- Ports: 7500 (paper), 7497 (live) — auto-detect with fallback
- Client IDs: 1 (interactive), 3 (params), 10-17 (alert scanners)
- All tws functions are async; engine threads create local tokio runtimes

### Scanner Data + Market Snapshots
The IB TWS scanner API (`ibapi::scanner::ScannerData`) only returns **contract
data** (rank, contract_details, leg) — not market data. For one-shot scans
(`cargo run -- scan`), `tws::fetch_snapshots()` makes snapshot `market_data`
requests (one per symbol, max 50, 3s timeout each) to fetch last price, bid,
ask, volume, and previous close. Change% is computed from last vs close.
Client ID 20 is used for snapshot connections. **Note:** Snapshots return no
price data when market is closed — this is expected TWS behavior.

Snapshots are NOT used during poll cycles (too many symbols would block the poll
thread). Poll alert rows get prices updated when enrichment or future snapshot
batching is added.

## Yahoo Finance API (IMPORTANT)
Yahoo Finance API **requires** cookie + crumb authentication on every request.
Without auth, requests return HTTP 200 with empty/null data — no error is raised.
This is a silent failure that is easy to miss. **Never call Yahoo Finance endpoints
without the cookie + crumb flow.**

### Auth Flow (implemented in `src/enrichment.rs`)
1. `GET https://fc.yahoo.com` → extract `set-cookie` response header
2. `GET https://query2.finance.yahoo.com/v1/test/getcrumb` with cookie → returns crumb string
3. All subsequent API calls must include:
   - `Cookie` request header (from step 1)
   - `&crumb=<url-encoded crumb>` query parameter (from step 2)
4. Use `query2.finance.yahoo.com` (not `query1`) for all endpoints

### Endpoints Used
- **Quote Summary**: `https://query2.finance.yahoo.com/v10/finance/quoteSummary/{symbol}?modules=summaryProfile,defaultKeyStatistics,financialData,price&crumb=...`
- **News Search**: `https://query2.finance.yahoo.com/v8/finance/search?q={symbol}&newsCount=5&quotesCount=0&crumb=...`

### Auth Caching
- The enrichment worker thread (`engine::spawn_enrichment_worker`) fetches auth once and reuses it for all symbols in the session
- `enrich_results()` (used by one-shot `scan` command) fetches auth once per batch
- `fetch_enrichment()` fetches auth per call (fallback, less efficient)

### Quick Verification
```bash
cargo run -- enrich AAPL    # Should show Name, Sector, Float, Short%, etc.
```
If enrichment returns all "-" or "none", auth is broken.
