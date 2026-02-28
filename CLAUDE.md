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
cargo run                      # launch TUI app
cargo run -- --help            # show CLI help
cargo test                     # run all tests
```

## CLI Testing Commands
The CLI has subcommands to test individual components without the full TUI:

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

# Launch full TUI
cargo run -- tui
```

## Project Structure (Modular)
```
src/
  main.rs          — CLI entry point with clap subcommands
  lib.rs           — re-exports all modules
  config.rs        — settings, .env loading, constants
  tws/
    mod.rs         — TWS client, connection, scanner subscriptions
    messages.rs    — IB API message encoding/decoding
  scanner.rs       — scanner logic, enrichment, filtering, table display
  history.rs       — Supabase persistence (sightings CRUD)
  enrichment.rs    — Yahoo Finance data fetching
  catalyst.rs      — news catalyst classification
  models.rs        — shared data types (ScanResult, AlertRow, Settings, etc.)
  tui/
    mod.rs         — Textual-style TUI with ratatui
    app.rs         — app state and event loop
    ui.rs          — rendering / layout
  error.rs         — error types
```

## Dependencies
- `tokio` — async runtime
- `reqwest` — HTTP client (Yahoo Finance, Supabase REST)
- `serde` / `serde_json` — serialization
- `clap` — CLI argument parsing
- `ratatui` + `crossterm` — TUI framework
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

## TUI Commands
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
Table `sightings` in hosted Postgres. Credentials in `.env`.
Columns: symbol (unique), first/last seen timestamps, scanners (comma-sep),
hit_count, last_price, change_pct, rvol, float_shares, catalyst, name, sector.

## Design Rules
- **Modular code** — each module handles one concern, testable in isolation
- **CLI for testing** — every module can be tested via CLI subcommands
- **Black background** — terminal aesthetic
- **Keep it simple** — ratatui for layout only, no over-engineering
- **Tests first** — unit tests for all pure logic (filtering, classification, enrichment)

## TWS API Protocol
Interactive Brokers TWS uses a binary protocol over TCP.
- Ports: 7500 (paper), 7497 (live) — auto-detect with fallback
- Client IDs: 1 (interactive), 3 (params), 10-17 (alert scanners)
- Market data type 4 = delayed frozen
