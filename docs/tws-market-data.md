# TWS Market Data: Architecture & Known Issues

## How Stocks Get Prices

The IB TWS scanner API returns **only contract data** (symbol, name, exchange, rank) — never market data. Prices must be fetched separately through one of two mechanisms:

### 1. Snapshots (One-Shot)

Used by: `cargo run -- scan`, poll cycles (top 50 symbols)

```
Scanner results (no prices)
    │
    ▼
fetch_snapshots() ── client_id 20 ── chunks of 10 ── 3s timeout each
    │
    ▼
ScanResult { last, close, bid, ask, volume, change_pct }
```

- Opens a **separate TWS connection** (client_id 20)
- Requests snapshot market data (`snapshot=true`) for each symbol
- Processes in concurrent chunks of 10 to avoid overwhelming TWS
- Capped at 50 symbols per batch
- Each snapshot has a 3-second timeout
- **Limitation**: Returns no data when market is closed (pre-market/after-hours snapshots may be empty)

### 2. Streaming (Continuous)

Used by: GUI and `alert` CLI mode for live price updates

```
Engine discovers new symbol
    │
    ▼
subscribe_market_data() ── sends MktDataRequest via mpsc
    │
    ▼
Market data worker (client_id 30, single persistent connection)
    │
    ▼
Per-symbol tokio task: client.market_data(&contract).subscribe().await
    │
    ▼
BgMessage::MarketDataTick { symbol, last, close, bid, ask, volume }
    │
    ▼
Engine tick() updates AlertRow prices
```

- Single persistent TWS connection (client_id 30)
- Each symbol gets its own async task streaming ticks
- Sub-second price updates while subscribed
- Sends `MarketDataTick` messages to engine via mpsc channel

## TWS Connection Client IDs

| Client ID | Purpose | Connection Lifetime |
|-----------|---------|-------------------|
| 0 | Port probe (`probe_port`) | Ephemeral |
| 1 | Interactive one-shot scan | Ephemeral |
| 3 | Scanner parameters fetch | Ephemeral |
| 10 | Poll scan (8 scanners) | Ephemeral per cycle |
| 20 | Snapshot market data | Ephemeral per cycle |
| 30 | Streaming market data | Persistent |

## Known Issues & Fixes

### Missing Prices on Poll-Discovered Stocks

**Symptom**: Stocks discovered by polling show "-" for Price and Chg% in the alert table.

**Root cause**: `run_poll_scan()` originally skipped snapshot fetching entirely, relying solely on the streaming worker for prices. Two problems with this:

1. **TWS market data line limit**: Most accounts allow ~100 concurrent streaming subscriptions. With 200+ discovered symbols, excess subscriptions silently fail — no error is raised, ticks simply never arrive.

2. **No initial price**: Even for successfully subscribed symbols, there's a delay before the first tick arrives. During this window, the stock shows no price.

**Fix**: `run_poll_scan()` now calls `fetch_snapshots()` (capped at 50 symbols) to provide immediate prices. Streaming continues to update prices for subscribed symbols afterward.

**Remaining limitation**: Only the top 50 symbols get snapshot prices per poll cycle. Symbols beyond 50 depend on streaming, which may hit the subscription limit. This is acceptable because the alert table is sorted by scanner hits — the most important stocks get prices first.

### TWS Connection Status Stuck on "Connected"

**Symptom**: Shutting down TWS while the app is running leaves the status bar showing "TWS: connected" indefinitely.

**Root cause**: `engine.connected_port` was only set to `Some(port)` on successful connections. When a poll/scan failed to connect (returning `port: None`), the field was never cleared — the stale `Some(port)` persisted.

**Fix**: `tick()` now sets `connected_port = None` when `BgMessage::ScanComplete` or `BgMessage::PollComplete` arrives with `port: None`. The status updates within one poll cycle (~15 seconds).

## Data Flow: Poll Cycle

```
run_poll_scanners() spawns background thread
    │
    ├── Connect to TWS (client_id 10)
    ├── Subscribe 8 scanners sequentially
    │     └── Each returns Vec<ScanResult> (no prices)
    ├── fetch_snapshots() on top 50 symbols (client_id 20)
    │     └── Populates last, close, volume, change_pct
    └── Send BgMessage::PollComplete { symbol_data, port }
            │
            ▼
        Engine tick()
            ├── Update connected_port (or clear if None)
            ├── Create AlertRow for new symbols (with snapshot prices)
            ├── Update prices for existing symbols
            ├── Subscribe new symbols to streaming (client_id 30)
            ├── Queue enrichment (Yahoo Finance)
            └── Emit EngineEvent::PollCycleComplete
```

## Enrichment Flow (Yahoo Finance)

After discovery, each symbol is queued for Yahoo Finance enrichment to fetch
fundamental data not available from TWS scanners.

```
Engine discovers new symbol (poll or scan)
    │
    ▼
queue_enrich(symbol, scanner_hits)  ── sends EnrichRequest via mpsc
    │
    ▼
Enrichment worker thread (priority queue by scanner_hits)
    ├── Check Supabase cache (15-min TTL)
    ├── If miss: fetch from Yahoo Finance (cookie+crumb auth)
    │     ├── quoteSummary: name, sector, industry, float, short%
    │     └── search: news headlines with timestamps
    └── Send BgMessage::EnrichComplete { symbol, data }
            │
            ▼
        Engine tick()
            ├── Write to Supabase (async, non-blocking)
            ├── Update matching AlertRow (alert view)
            └── Emit EngineEvent::EnrichComplete { symbol, data }
                    │
                    ▼
                App handle_engine_event()
                    ├── Update matching AlertRow (already done by engine)
                    └── Update matching ScanResult (scanner view detail panel)
```

**Key points**:
- Enrichment is queued for **both** poll-discovered and one-shot scan results
- Higher scanner_hits = higher priority in the enrichment queue
- `EngineEvent::EnrichComplete` carries the full `EnrichmentData` so both
  `alert_rows` (alert view) and `scan_results` (scanner view) get updated
- Yahoo auth (cookie+crumb) is fetched once per session and reused
- Supabase caches enrichment for 15 minutes to avoid redundant API calls

## Debugging Tips

```bash
# Check if snapshots return data (market must be open)
cargo run -- scan gain --rows 5

# If prices show "-", market may be closed or TWS not connected
# Check var/scanner.log for:
#   "market data snapshots" — how many snapshots succeeded
#   "market data subscribe failed" — streaming subscription errors
#   "Poll scan connect failed" — can't reach TWS at all

# Verify TWS connection
cargo run -- scan list   # Should return scanner categories
```
