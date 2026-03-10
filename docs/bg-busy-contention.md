# Background Busy Flag Contention

## Summary

One-shot scans from the scanner view were silently blocked because they shared
a single `poll_busy` mutex flag with the 15-second poll cycle.

## Timeline

1. Poll scan initially only ran 8 TWS scanners (~1.5s total) — `poll_busy` was
   true for a short window, leaving plenty of time for user scans.

2. We added `fetch_snapshots()` to `run_poll_scan()` to fix missing prices.
   This fetches market data for up to 50 symbols in chunks of 10, with a
   3-second timeout per symbol.

3. **Worst case**: 5 chunks × 3s = 15 seconds — the entire poll interval.
   `poll_busy` was now true for up to 100% of the time.

4. User clicks a scanner → `cmd_scan()` checks `poll_busy` → always true →
   scan rejected with "Background operation in progress, please wait..."

5. The error message went to `output_lines`, but the scanner view was showing
   the results panel (empty), not the output panel. So the user saw nothing.

## Root Cause

```
Poll cycle (15s interval)
├── poll_busy = true
├── run_poll_scan: 8 scanners (~1.5s)
├── fetch_snapshots: 50 symbols (~5-15s)  ← NEW, pushed duration to ~15s
├── poll_busy = false
└── next poll in 15s...

User scan (blocked)
├── cmd_scan checks poll_busy → TRUE
├── "Background operation in progress"
└── scan never runs
```

A single boolean guarded three unrelated operations:
- Poll scanner subscriptions (client_id 10)
- One-shot scanner subscriptions (client_id 1)
- Scanner parameter fetches (client_id 3)

These use different TWS client_ids and don't actually conflict at the TWS
protocol level. The shared flag was an unnecessary bottleneck.

## Fix

Split into two independent flags:

| Flag | Guards | Used By |
|------|--------|---------|
| `poll_busy` | Poll cycles, list fetches | `run_poll_scanners()`, `start_list()` |
| `scan_busy` | One-shot scans | `start_scan()` |

Each also uses a separate TWS snapshot client_id to avoid connection conflicts:

| Client ID | Used For |
|-----------|----------|
| 20 | Snapshots for one-shot scans |
| 21 | Snapshots for poll scans |

```
Poll cycle                          User scan
├── poll_busy = true                  ├── scan_busy = true
├── scanners (client 10)            ├── scanner (client 1)
├── snapshots (client 21)           ├── snapshots (client 20)
├── poll_busy = false                 ├── scan_busy = false
└── runs every 15s                  └── runs on demand
         │                                    │
         └── independent, no contention ──────┘
```

## Lesson

When adding work to a background task (like `fetch_snapshots` to polling),
check whether the task's duration now approaches or exceeds its interval.
If it does, any other operation sharing the same busy flag will starve.

Separate busy flags should be used for operations that:
1. Use different TWS client_ids (no protocol conflict)
2. Are triggered by different sources (user action vs timer)
3. Have different priority (user actions should not wait for background work)
