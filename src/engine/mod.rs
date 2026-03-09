use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::mpsc;
use std::time::Duration;

use tracing::{info, warn};

use crate::enrichment::{fetch_enrichment_with_auth, fetch_yahoo_auth, EnrichmentData, YahooAuth};
use crate::history::SupabaseClient;
use crate::models::*;
use crate::tws;

/// Message from a background TWS operation.
pub enum BgMessage {
    ScanComplete {
        scanner_code: String,
        results: Vec<ScanResult>,
        port: Option<u16>,
    },
    ListComplete {
        xml: Option<String>,
        group: Option<String>,
    },
    PollComplete {
        symbol_data: HashMap<String, ScanResult>,
        symbol_scanners: HashMap<String, Vec<String>>,
        port: Option<u16>,
        scanners_run: usize,
        elapsed_secs: f64,
    },
    EnrichComplete {
        symbol: String,
        data: EnrichmentData,
    },
    /// Real-time market data tick from the streaming thread.
    MarketDataTick {
        symbol: String,
        last: Option<f64>,
        close: Option<f64>,
        bid: Option<f64>,
        ask: Option<f64>,
        volume: Option<i64>,
    },
}

/// Request to enrich a symbol, ordered by scanner_hits (higher = higher priority).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EnrichRequest {
    pub symbol: String,
    pub scanner_hits: u32,
}

impl Ord for EnrichRequest {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.scanner_hits.cmp(&other.scanner_hits)
    }
}

impl PartialOrd for EnrichRequest {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Events emitted by the engine for consumers (CLI or GUI).
pub enum EngineEvent {
    ScanComplete {
        scanner_code: String,
        results: Vec<ScanResult>,
    },
    ListComplete {
        xml: Option<String>,
        group: Option<String>,
    },
    PollCycleComplete {
        total_stocks: usize,
        new_symbols: Vec<String>,
        scanners_run: usize,
        elapsed_secs: f64,
    },
    EnrichComplete {
        symbol: String,
        data: EnrichmentData,
    },
    PortDiscovered {
        port: u16,
    },
}

/// Request to subscribe to streaming market data for a symbol.
#[derive(Debug, Clone)]
pub struct MktDataRequest {
    pub symbol: String,
    pub currency: String,
}

/// Core alert engine — business logic shared by GUI and CLI.
pub struct AlertEngine {
    pub settings: Settings,
    pub alert_rows: Vec<AlertRow>,
    pub alert_seen: HashSet<String>,
    pub streaming_set: HashSet<String>,
    pub polling: bool,
    pub connected_port: Option<u16>,
    pub db: Option<SupabaseClient>,
    pub bg_tx: mpsc::Sender<BgMessage>,
    pub bg_rx: mpsc::Receiver<BgMessage>,
    pub bg_busy: bool,
    pub scan_busy: bool,
    pub enrich_tx: mpsc::Sender<EnrichRequest>,
    pub mktdata_tx: Option<mpsc::Sender<MktDataRequest>>,
}

impl AlertEngine {
    pub fn new(
        enrich_tx: mpsc::Sender<EnrichRequest>,
        settings: Settings,
        db: Option<SupabaseClient>,
    ) -> Self {
        let (bg_tx, bg_rx) = mpsc::channel();
        Self {
            settings,
            alert_rows: Vec::new(),
            alert_seen: HashSet::new(),
            streaming_set: HashSet::new(),
            polling: false,
            connected_port: None,
            db,
            bg_tx,
            bg_rx,
            bg_busy: false,
            scan_busy: false,
            enrich_tx,
            mktdata_tx: None,
        }
    }

    /// Subscribe a symbol to streaming market data (if not already subscribed).
    pub fn subscribe_market_data(&mut self, symbol: &str, currency: &str) {
        if self.streaming_set.contains(symbol) {
            return;
        }
        if let Some(ref tx) = self.mktdata_tx {
            let _ = tx.send(MktDataRequest {
                symbol: symbol.to_string(),
                currency: currency.to_string(),
            });
            self.streaming_set.insert(symbol.to_string());
        }
    }

    /// Queue enrichment for a symbol if the channel is available.
    pub fn queue_enrich(&self, symbol: &str, scanner_hits: u32) {
        let _ = self.enrich_tx.send(EnrichRequest {
            symbol: symbol.to_string(),
            scanner_hits,
        });
    }

    /// Start a one-shot scan in a background thread.
    pub fn start_scan(
        &mut self,
        code: &str,
        rows: u32,
        min_price: Option<f64>,
        max_price: Option<f64>,
    ) {
        if self.scan_busy {
            return;
        }
        self.scan_busy = true;

        let ports: Vec<u16> = self
            .settings
            .port
            .map(|p| vec![p])
            .unwrap_or_else(|| DEFAULT_PORTS.to_vec());
        let host = self.settings.host.clone();
        let tx = self.bg_tx.clone();
        let code = code.to_string();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let (results, port) = rt.block_on(
                tws::run_scan(&code, &host, &ports, 1, rows, min_price, max_price),
            );
            let _ = tx.send(BgMessage::ScanComplete {
                scanner_code: code,
                results,
                port,
            });
        });
    }

    /// Start a list/scanner-params fetch in a background thread.
    pub fn start_list(&mut self, group: Option<String>) {
        if self.bg_busy {
            return;
        }
        self.bg_busy = true;

        let ports: Vec<u16> = self
            .settings
            .port
            .map(|p| vec![p])
            .unwrap_or_else(|| DEFAULT_PORTS.to_vec());
        let host = self.settings.host.clone();
        let tx = self.bg_tx.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let xml = rt.block_on(tws::fetch_scanner_params(&host, &ports, 3));
            let _ = tx.send(BgMessage::ListComplete { xml, group });
        });
    }

    /// Start polling. Returns true if first poll was kicked off.
    pub fn poll_on(&mut self) -> bool {
        if self.polling {
            return false;
        }
        self.polling = true;
        self.run_poll_scanners();
        true
    }

    /// Stop polling.
    pub fn poll_off(&mut self) {
        self.polling = false;
    }

    /// Clear seen-set and alert rows, send sentinel to enrichment worker.
    pub fn poll_clear(&mut self) -> usize {
        let count = self.alert_seen.len();
        self.alert_seen.clear();
        self.alert_rows.clear();
        self.streaming_set.clear();
        let _ = self.enrich_tx.send(EnrichRequest {
            symbol: String::new(),
            scanner_hits: 0,
        });
        // Send sentinel to market data worker to cancel all subscriptions
        if let Some(ref tx) = self.mktdata_tx {
            let _ = tx.send(MktDataRequest {
                symbol: String::new(),
                currency: String::new(),
            });
        }
        count
    }

    /// Spawn the multi-scanner poll in a background thread.
    pub fn run_poll_scanners(&mut self) {
        if self.bg_busy {
            return;
        }
        self.bg_busy = true;

        let ports: Vec<u16> = self
            .settings
            .port
            .map(|p| vec![p])
            .unwrap_or_else(|| DEFAULT_PORTS.to_vec());
        let host = self.settings.host.clone();
        let tx = self.bg_tx.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let start = std::time::Instant::now();

            let (symbol_scanners, symbol_data, connected_port) = rt.block_on(
                tws::run_poll_scan(ALERT_SCANNERS, &host, &ports, 10, 50, Some(1.0), Some(20.0)),
            );

            let scanners_run = ALERT_SCANNERS.len();
            let elapsed_secs = start.elapsed().as_secs_f64();
            info!(unique_stocks = symbol_data.len(), scanners_run, elapsed_secs, "poll scan complete");

            let _ = tx.send(BgMessage::PollComplete {
                symbol_data,
                symbol_scanners,
                port: connected_port,
                scanners_run,
                elapsed_secs,
            });
        });
    }

    /// Drain bg_rx, process messages, return events for consumers.
    pub fn tick(&mut self, rt: &tokio::runtime::Handle) -> Vec<EngineEvent> {
        let mut events = Vec::new();

        while let Ok(msg) = self.bg_rx.try_recv() {
            match msg {
                BgMessage::ScanComplete {
                    scanner_code,
                    results,
                    port,
                } => {
                    if let Some(p) = port {
                        self.connected_port = Some(p);
                        events.push(EngineEvent::PortDiscovered { port: p });
                    } else {
                        self.connected_port = None;
                    }
                    self.scan_busy = false;
                    // Queue enrichment for scan results
                    for r in &results {
                        self.queue_enrich(&r.symbol, 1);
                    }
                    events.push(EngineEvent::ScanComplete {
                        scanner_code,
                        results,
                    });
                }
                BgMessage::ListComplete { xml, group } => {
                    self.bg_busy = false;
                    events.push(EngineEvent::ListComplete { xml, group });
                }
                BgMessage::PollComplete {
                    symbol_data,
                    symbol_scanners,
                    port,
                    scanners_run,
                    elapsed_secs,
                } => {
                    if let Some(p) = port {
                        self.connected_port = Some(p);
                        events.push(EngineEvent::PortDiscovered { port: p });
                    } else {
                        self.connected_port = None;
                    }

                    // Write to Supabase (background, non-blocking)
                    if let Some(ref self_db) = self.db {
                        let batch: HashMap<String, (serde_json::Value, Vec<String>)> = symbol_data
                            .iter()
                            .map(|(sym, r)| {
                                let data = serde_json::json!({
                                    "last": r.last,
                                    "change_pct": r.change_pct,
                                    "rvol": r.rvol,
                                    "float_shares": r.float_shares,
                                    "catalyst": r.catalyst,
                                    "name": r.name,
                                    "sector": r.sector,
                                });
                                (
                                    sym.clone(),
                                    (
                                        data,
                                        symbol_scanners.get(sym).cloned().unwrap_or_default(),
                                    ),
                                )
                            })
                            .collect();
                        let mut db = self_db.clone();
                        rt.spawn(async move {
                            if let Err(e) = db.record_stocks_batch(&batch).await {
                                warn!("Supabase write error: {e}");
                            }
                        });
                    }

                    // Detect new symbols
                    let now = chrono::Local::now().format("%H:%M:%S").to_string();
                    let new_syms: Vec<String> = symbol_data
                        .keys()
                        .filter(|s| !self.alert_seen.contains(*s))
                        .cloned()
                        .collect();

                    let total_stocks = symbol_data.len();

                    for sym in &new_syms {
                        self.alert_seen.insert(sym.clone());
                        if let Some(r) = symbol_data.get(sym) {
                            let hits = symbol_scanners
                                .get(sym)
                                .map(|s| s.len() as u32)
                                .unwrap_or(0);
                            let chg = r.change_pct.map(|c| format!("{c:+.1}%")).unwrap_or("-".into());
                            info!(symbol = %sym, hits, change = %chg, "new alert");
                            let scanner_list = symbol_scanners
                                .get(sym)
                                .cloned()
                                .unwrap_or_default();
                            self.alert_rows.push(AlertRow {
                                symbol: sym.clone(),
                                alert_time: now.clone(),
                                last: r.last,
                                change_pct: r.change_pct,
                                volume: r.volume,
                                rvol: None,
                                float_shares: None,
                                short_pct: None,
                                name: None,
                                sector: None,
                                industry: None,
                                catalyst: None,
                                catalyst_time: None,
                                scanner_hits: hits,
                                scanners: scanner_list,
                                news_headlines: Vec::new(),
                                enriched: false,
                                avg_volume: None,
                            });
                            // Subscribe to streaming market data for live price updates
                            self.subscribe_market_data(sym, &r.currency);
                            self.queue_enrich(sym, hits);
                        }
                    }

                    // Update price/volume for already-seen symbols
                    for row in &mut self.alert_rows {
                        if let Some(r) = symbol_data.get(&row.symbol) {
                            if r.last.is_some() {
                                row.last = r.last;
                            }
                            if r.change_pct.is_some() {
                                row.change_pct = r.change_pct;
                            }
                            if r.volume.is_some() {
                                row.volume = r.volume;
                            }
                            // Update scanner hits and list
                            if let Some(new_scanners) = symbol_scanners.get(&row.symbol) {
                                for s in new_scanners {
                                    if !row.scanners.contains(s) {
                                        row.scanners.push(s.clone());
                                    }
                                }
                                let hits = row.scanners.len() as u32;
                                if hits > row.scanner_hits {
                                    row.scanner_hits = hits;
                                }
                            }
                        }
                    }

                    // Sort alert rows
                    self.alert_rows.sort_by(|a, b| {
                        b.scanner_hits
                            .cmp(&a.scanner_hits)
                            .then_with(|| {
                                b.change_pct
                                    .unwrap_or(0.0)
                                    .partial_cmp(&a.change_pct.unwrap_or(0.0))
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            })
                    });

                    self.bg_busy = false;
                    events.push(EngineEvent::PollCycleComplete {
                        total_stocks,
                        new_symbols: new_syms,
                        scanners_run,
                        elapsed_secs,
                    });
                }
                BgMessage::EnrichComplete { symbol, data } => {
                    // Write enrichment to Supabase (background, non-blocking)
                    if let Some(ref self_db) = self.db {
                        let headlines_json = serde_json::to_string(&data.news_headlines)
                            .unwrap_or_else(|_| "[]".to_string());
                        let supa_data = serde_json::json!({
                            "name": &data.name,
                            "sector": &data.sector,
                            "catalyst": &data.catalyst,
                            "catalyst_time": data.catalyst_time,
                            "float_shares": data.float_shares,
                            "industry": &data.industry,
                            "short_pct": data.short_pct,
                            "avg_volume": data.avg_volume,
                            "news_headlines": headlines_json,
                            "enriched_at": chrono::Utc::now().to_rfc3339(),
                        });
                        let batch: HashMap<String, (serde_json::Value, Vec<String>)> =
                            [(symbol.clone(), (supa_data, vec![]))]
                                .into_iter()
                                .collect();
                        let mut db = self_db.clone();
                        rt.spawn(async move {
                            if let Err(e) = db.record_stocks_batch(&batch).await {
                                warn!("Supabase enrich write error: {e}");
                            }
                        });
                    }

                    let cat = data.catalyst.as_deref().unwrap_or("-");
                    let float_str = data.float_shares
                        .map(|f| if f >= 1e9 { format!("{:.1}B", f / 1e9) } else if f >= 1e6 { format!("{:.1}M", f / 1e6) } else { format!("{:.0}", f) })
                        .unwrap_or("-".into());
                    info!(symbol = %symbol, catalyst = cat, float = %float_str, "enriched");

                    // Update matching AlertRow
                    let data_clone = data.clone();
                    if let Some(row) =
                        self.alert_rows.iter_mut().find(|r| r.symbol == symbol)
                    {
                        row.name = data.name;
                        row.sector = data.sector;
                        row.industry = data.industry;
                        row.float_shares = data.float_shares;
                        row.short_pct = data.short_pct;
                        row.catalyst = data.catalyst;
                        row.catalyst_time = data.catalyst_time;
                        row.news_headlines = data.news_headlines;
                        row.avg_volume = data.avg_volume;
                        if let (Some(vol), Some(avg)) = (row.volume, data.avg_volume) {
                            if avg > 0 {
                                row.rvol = Some(vol as f64 / avg as f64);
                            }
                        }
                        row.enriched = true;
                    }

                    events.push(EngineEvent::EnrichComplete { symbol, data: data_clone });
                }
                BgMessage::MarketDataTick {
                    symbol,
                    last,
                    close,
                    bid: _,
                    ask: _,
                    volume,
                } => {
                    if let Some(row) =
                        self.alert_rows.iter_mut().find(|r| r.symbol == symbol)
                    {
                        if let Some(l) = last {
                            row.last = Some(l);
                        }
                        if let Some(v) = volume {
                            row.volume = Some(v);
                            // Recompute rvol if avg_volume is known
                            if let Some(avg) = row.avg_volume {
                                if avg > 0 {
                                    row.rvol = Some(v as f64 / avg as f64);
                                }
                            }
                        }
                        // Compute change_pct from last and close
                        let effective_close = close.or_else(|| {
                            // Use stored change_pct to back-derive close if we don't have it
                            None
                        });
                        if let (Some(l), Some(c)) = (row.last, effective_close) {
                            if c > 0.0 {
                                row.change_pct = Some((l - c) / c * 100.0);
                            }
                        }
                    }
                }
            }
        }

        events
    }

    /// Probe TWS to discover the connected port.
    pub fn probe_port(&mut self) {
        let ports: Vec<u16> = self
            .settings
            .port
            .map(|p| vec![p])
            .unwrap_or_else(|| DEFAULT_PORTS.to_vec());
        let rt = tokio::runtime::Runtime::new().unwrap();
        if let Some(port) = rt.block_on(tws::probe_port(&self.settings.host, &ports)) {
            self.connected_port = Some(port);
        }
    }

    /// Load today's tws_scans from Supabase and populate alert state.
    /// Returns (loaded_count, needs_enrichment_count).
    pub fn init_from_tws_scans(&mut self, rt: &tokio::runtime::Handle) -> (usize, usize) {
        if let Some(ref db) = self.db {
            let result = rt.block_on(db.get_today());
            if let Err(ref e) = result {
                warn!("Failed to load today's scans from Supabase: {e}");
                eprintln!("Supabase error: {e}");
            }
            if let Ok(today) = result {
                let loaded = today.len();
                let mut needs_enrich = 0usize;
                for s in &today {
                    self.alert_seen.insert(s.symbol.clone());
                    let scanners_str = &s.scanners;
                    let n_scans = scanners_str.split(',').count() as u32;

                    // Check if enrichment is fresh (within cache TTL)
                    let enrichment_fresh = s.enriched_at.as_ref().map_or(false, |ea| {
                        chrono::DateTime::parse_from_rfc3339(ea)
                            .map(|dt| {
                                let age = chrono::Utc::now()
                                    .signed_duration_since(dt.with_timezone(&chrono::Utc));
                                age < chrono::Duration::from_std(ENRICH_CACHE_TTL)
                                    .unwrap_or(chrono::Duration::zero())
                            })
                            .unwrap_or(false)
                    });

                    // Deserialize news_headlines with backwards compat for old string-only format
                    let news_headlines: Vec<crate::models::NewsHeadline> = s
                        .news_headlines
                        .as_deref()
                        .and_then(|h| {
                            // Try new format first: Vec<NewsHeadline>
                            serde_json::from_str::<Vec<crate::models::NewsHeadline>>(h)
                                .ok()
                                .or_else(|| {
                                    // Fallback: old Vec<String> format
                                    serde_json::from_str::<Vec<String>>(h).ok().map(|titles| {
                                        titles.into_iter().map(|title| crate::models::NewsHeadline {
                                            title,
                                            published: None,
                                        }).collect()
                                    })
                                })
                        })
                        .unwrap_or_default();

                    self.alert_rows.push(AlertRow {
                        symbol: s.symbol.clone(),
                        alert_time: crate::history::local_time_str(&s.first_seen),
                        last: s.last_price,
                        change_pct: s.change_pct,
                        volume: None,
                        rvol: s.rvol,
                        float_shares: s.float_shares,
                        short_pct: s.short_pct,
                        name: s.name.clone(),
                        sector: s.sector.clone(),
                        industry: s.industry.clone(),
                        catalyst: s.catalyst.clone(),
                        catalyst_time: s.catalyst_time,
                        scanner_hits: n_scans,
                        scanners: scanners_str.split(',').filter(|s| !s.is_empty()).map(String::from).collect(),
                        news_headlines,
                        enriched: enrichment_fresh,
                        avg_volume: s.avg_volume,
                    });
                    if !enrichment_fresh {
                        needs_enrich += 1;
                        self.queue_enrich(&s.symbol, n_scans);
                    }
                }
                info!(loaded, needs_enrich, "tws_scans loaded");
                return (loaded, needs_enrich);
            }
        }
        (0, 0)
    }
}

/// Cache TTL for enrichment data (15 minutes).
const ENRICH_CACHE_TTL: Duration = Duration::from_secs(15 * 60);

/// Spawn the enrichment worker thread with optional Supabase cache.
pub fn spawn_enrichment_worker(
    bg_tx: mpsc::Sender<BgMessage>,
    enrich_rx: mpsc::Receiver<EnrichRequest>,
    rt_handle: tokio::runtime::Handle,
    db: Option<SupabaseClient>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let client = reqwest::Client::new();
        let mut heap = BinaryHeap::<EnrichRequest>::new();
        let mut enriched_set = HashSet::<String>::new();
        let mut yahoo_auth: Option<YahooAuth> = None;

        loop {
            // Drain all pending requests into the priority queue
            loop {
                match enrich_rx.try_recv() {
                    Ok(req) => {
                        if req.symbol.is_empty() {
                            // Sentinel: clear enriched set
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

            // Process highest-priority item
            if let Some(req) = heap.pop() {
                if enriched_set.contains(&req.symbol) {
                    continue;
                }
                enriched_set.insert(req.symbol.clone());

                // Try Supabase cache first
                let cached = db.as_ref().and_then(|db| {
                    rt_handle
                        .block_on(db.get_enrichment_cache(&req.symbol, ENRICH_CACHE_TTL))
                });

                let data = if let Some(cached_data) = cached {
                    info!(symbol = %req.symbol, "enrichment cache hit");
                    cached_data
                } else {
                    // Fetch or reuse Yahoo auth
                    if yahoo_auth.is_none() {
                        yahoo_auth = rt_handle.block_on(fetch_yahoo_auth(&client)).ok();
                        if yahoo_auth.is_none() {
                            warn!("Yahoo auth failed, skipping enrichment");
                        }
                    }
                    if let Some(ref auth) = yahoo_auth {
                        info!(symbol = %req.symbol, priority = req.scanner_hits, "enriching via Yahoo");
                        rt_handle.block_on(fetch_enrichment_with_auth(&client, &req.symbol, auth))
                    } else {
                        EnrichmentData::default()
                    }
                };

                let _ = bg_tx.send(BgMessage::EnrichComplete {
                    symbol: req.symbol,
                    data,
                });
            } else {
                // Nothing to do — block until a request arrives
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
    })
}

/// Spawn the market data streaming worker thread.
///
/// Holds a persistent TWS connection and subscribes to real-time market data
/// for symbols sent via `mktdata_rx`. Each subscription gets its own tokio task
/// that forwards price/volume ticks to the engine via `bg_tx`.
pub fn spawn_market_data_worker(
    bg_tx: mpsc::Sender<BgMessage>,
    mktdata_rx: mpsc::Receiver<MktDataRequest>,
    host: String,
    ports: Vec<u16>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        use ibapi::market_data::realtime::TickTypes;
        use std::sync::Arc;

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            // Connect to TWS (client_id 30 for streaming)
            let ports_ref = if ports.is_empty() { DEFAULT_PORTS } else { &ports };
            let mut client_opt = None;
            for &port in ports_ref {
                let addr = format!("{host}:{port}");
                match ibapi::Client::connect(&addr, 30).await {
                    Ok(c) => {
                        info!(port, "market data stream connected");
                        client_opt = Some(c);
                        break;
                    }
                    Err(e) => {
                        warn!(port, "market data stream connect failed: {e}");
                    }
                }
            }
            let client = match client_opt {
                Some(c) => Arc::new(c),
                None => {
                    warn!("market data worker: could not connect to TWS");
                    return;
                }
            };

            let mut subscribed: HashSet<String> = HashSet::new();

            loop {
                // Drain new subscription requests
                loop {
                    match mktdata_rx.try_recv() {
                        Ok(req) => {
                            if req.symbol.is_empty() {
                                // Sentinel: clear tracked set (tasks will end naturally
                                // when their subscriptions are cancelled by TWS)
                                subscribed.clear();
                                continue;
                            }
                            if subscribed.contains(&req.symbol) {
                                continue;
                            }
                            subscribed.insert(req.symbol.clone());

                            // Spawn an async task per symbol to stream ticks
                            let client = Arc::clone(&client);
                            let tx = bg_tx.clone();
                            let symbol = req.symbol.clone();
                            let currency = req.currency.clone();
                            tokio::spawn(async move {
                                let contract = ibapi::contracts::Contract::stock(&symbol);
                                let cur = if currency.is_empty() { "USD" } else { &currency };
                                let contract = ibapi::contracts::Contract {
                                    currency: ibapi::contracts::Currency::from(cur),
                                    ..contract.build()
                                };

                                let mut subscription = match client
                                    .market_data(&contract)
                                    .subscribe()
                                    .await
                                {
                                    Ok(s) => s,
                                    Err(e) => {
                                        warn!(symbol = %symbol, "market data subscribe failed: {e}");
                                        return;
                                    }
                                };

                                info!(symbol = %symbol, "streaming market data subscribed");
                                let mut stored_close: Option<f64> = None;

                                while let Some(result) = subscription.next().await {
                                    let tick = match result {
                                        Ok(t) => t,
                                        Err(_) => break,
                                    };

                                    let mut last = None;
                                    let mut close = None;
                                    let mut bid = None;
                                    let mut ask = None;
                                    let mut volume = None;

                                    match tick {
                                        TickTypes::Price(tp) => match tp.tick_type {
                                            ibapi::contracts::tick_types::TickType::Last => last = Some(tp.price),
                                            ibapi::contracts::tick_types::TickType::Close => {
                                                close = Some(tp.price);
                                                stored_close = close;
                                            }
                                            ibapi::contracts::tick_types::TickType::Bid => bid = Some(tp.price),
                                            ibapi::contracts::tick_types::TickType::Ask => ask = Some(tp.price),
                                            _ => continue,
                                        },
                                        TickTypes::PriceSize(tp) => match tp.price_tick_type {
                                            ibapi::contracts::tick_types::TickType::Last => last = Some(tp.price),
                                            ibapi::contracts::tick_types::TickType::Close => {
                                                close = Some(tp.price);
                                                stored_close = close;
                                            }
                                            ibapi::contracts::tick_types::TickType::Bid => bid = Some(tp.price),
                                            ibapi::contracts::tick_types::TickType::Ask => ask = Some(tp.price),
                                            _ => continue,
                                        },
                                        TickTypes::Size(ts) => {
                                            if ts.tick_type == ibapi::contracts::tick_types::TickType::Volume {
                                                volume = Some(ts.size as i64);
                                            } else {
                                                continue;
                                            }
                                        }
                                        _ => continue,
                                    }

                                    let _ = tx.send(BgMessage::MarketDataTick {
                                        symbol: symbol.clone(),
                                        last,
                                        close: close.or(stored_close),
                                        bid,
                                        ask,
                                        volume,
                                    });
                                }
                            });
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return,
                    }
                }

                // Sleep briefly before checking for new requests
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BinaryHeap;

    #[test]
    fn test_enrich_request_priority_ordering() {
        let low = EnrichRequest {
            symbol: "LOW".to_string(),
            scanner_hits: 1,
        };
        let mid = EnrichRequest {
            symbol: "MID".to_string(),
            scanner_hits: 4,
        };
        let high = EnrichRequest {
            symbol: "HIGH".to_string(),
            scanner_hits: 8,
        };
        assert!(high > mid);
        assert!(mid > low);
        assert!(high > low);

        let mut heap = BinaryHeap::new();
        heap.push(low);
        heap.push(high);
        heap.push(mid);
        assert_eq!(heap.pop().unwrap().symbol, "HIGH");
        assert_eq!(heap.pop().unwrap().symbol, "MID");
        assert_eq!(heap.pop().unwrap().symbol, "LOW");
    }

    #[test]
    fn test_engine_initial_state() {
        let (tx, _rx) = mpsc::channel();
        let engine = AlertEngine::new(tx, Settings::default(), None);
        assert!(engine.alert_rows.is_empty());
        assert!(engine.alert_seen.is_empty());
        assert!(!engine.polling);
        assert!(!engine.bg_busy);
        assert!(engine.connected_port.is_none());
    }

    #[test]
    fn test_poll_on_off() {
        let (tx, _rx) = mpsc::channel();
        let mut engine = AlertEngine::new(tx, Settings::default(), None);
        // poll_on returns true first time (but bg thread will fail to connect — that's ok)
        assert!(!engine.polling);
        engine.polling = true; // simulate
        engine.poll_off();
        assert!(!engine.polling);
    }

    #[test]
    fn test_poll_clear() {
        let (tx, _rx) = mpsc::channel();
        let mut engine = AlertEngine::new(tx, Settings::default(), None);
        engine.alert_seen.insert("AAPL".to_string());
        engine.alert_seen.insert("TSLA".to_string());
        engine.alert_rows.push(AlertRow {
            symbol: "AAPL".to_string(),
            alert_time: "10:00:00".to_string(),
            last: Some(150.0),
            change_pct: Some(5.0),
            volume: None,
            rvol: None,
            float_shares: None,
            short_pct: None,
            name: None,
            sector: None,
            industry: None,
            catalyst: None,
            catalyst_time: None,
            scanner_hits: 3,
            scanners: vec!["HOT_BY_VOLUME".into(), "TOP_PERC_GAIN".into(), "MOST_ACTIVE".into()],
            news_headlines: Vec::new(),
            enriched: false,
            avg_volume: None,
        });
        let count = engine.poll_clear();
        assert_eq!(count, 2);
        assert!(engine.alert_seen.is_empty());
        assert!(engine.alert_rows.is_empty());
    }

    #[test]
    fn test_tick_empty() {
        let (tx, _rx) = mpsc::channel();
        let mut engine = AlertEngine::new(tx, Settings::default(), None);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let events = engine.tick(rt.handle());
        assert!(events.is_empty());
    }
}
