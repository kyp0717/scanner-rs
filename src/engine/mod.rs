use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::mpsc;
use std::time::Duration;

use tracing::{info, warn};

use crate::enrichment::{fetch_enrichment, EnrichmentData};
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

/// Events emitted by the engine for consumers (CLI or TUI).
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
    },
    PortDiscovered {
        port: u16,
    },
}

/// Core alert engine — business logic shared by TUI and CLI.
pub struct AlertEngine {
    pub settings: Settings,
    pub alert_rows: Vec<AlertRow>,
    pub alert_seen: HashSet<String>,
    pub polling: bool,
    pub connected_port: Option<u16>,
    pub db: Option<SupabaseClient>,
    pub bg_tx: mpsc::Sender<BgMessage>,
    pub bg_rx: mpsc::Receiver<BgMessage>,
    pub bg_busy: bool,
    pub enrich_tx: mpsc::Sender<EnrichRequest>,
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
            polling: false,
            connected_port: None,
            db,
            bg_tx,
            bg_rx,
            bg_busy: false,
            enrich_tx,
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
        let code = code.to_string();

        std::thread::spawn(move || {
            let (results, port) =
                tws::run_scan(&code, &host, &ports, 1, rows, min_price, max_price);
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
            let xml = tws::fetch_scanner_params(&host, &ports, 3);
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
        let _ = self.enrich_tx.send(EnrichRequest {
            symbol: String::new(),
            scanner_hits: 0,
        });
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
            let start = std::time::Instant::now();
            let mut symbol_data: HashMap<String, ScanResult> = HashMap::new();
            let mut symbol_scanners: HashMap<String, Vec<String>> = HashMap::new();
            let mut connected_port = None;
            let mut scanners_run = 0usize;

            for (i, &(code, cid)) in ALERT_SCANNERS.iter().enumerate() {
                let (results, port) =
                    tws::run_scan(code, &host, &ports, cid, 50, Some(1.0), Some(20.0));
                if connected_port.is_none() {
                    connected_port = port;
                }
                let count = results.len();
                scanners_run += 1;

                for r in results {
                    let sym = r.symbol.clone();
                    symbol_scanners
                        .entry(sym.clone())
                        .or_default()
                        .push(code.to_string());
                    symbol_data.entry(sym).or_insert(r);
                }
                info!(scanner = i + 1, total = ALERT_SCANNERS.len(), code, count, "scanner results");
            }

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
                    }
                    self.bg_busy = false;
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
                    }

                    // Write to Supabase
                    if let Some(ref mut db) = self.db {
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
                        match rt.block_on(db.record_stocks_batch(&batch)) {
                            Ok(_) => {}
                            Err(e) => warn!("Supabase write error: {e}"),
                        }
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
                                scanner_hits: hits,
                                news_headlines: Vec::new(),
                                enriched: false,
                                avg_volume: None,
                            });
                            self.queue_enrich(sym, hits);
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
                    // Write enrichment to Supabase
                    if let Some(ref mut db) = self.db {
                        let headlines_json = serde_json::to_string(&data.news_headlines)
                            .unwrap_or_else(|_| "[]".to_string());
                        let supa_data = serde_json::json!({
                            "name": &data.name,
                            "sector": &data.sector,
                            "catalyst": &data.catalyst,
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
                        match rt.block_on(db.record_stocks_batch(&batch)) {
                            Ok(_) => {}
                            Err(e) => warn!("Supabase enrich write error: {e}"),
                        }
                    }

                    let cat = data.catalyst.as_deref().unwrap_or("-");
                    let float_str = data.float_shares
                        .map(|f| if f >= 1e9 { format!("{:.1}B", f / 1e9) } else if f >= 1e6 { format!("{:.1}M", f / 1e6) } else { format!("{:.0}", f) })
                        .unwrap_or("-".into());
                    info!(symbol = %symbol, catalyst = cat, float = %float_str, "enriched");

                    // Update matching AlertRow
                    if let Some(row) =
                        self.alert_rows.iter_mut().find(|r| r.symbol == symbol)
                    {
                        row.name = data.name;
                        row.sector = data.sector;
                        row.industry = data.industry;
                        row.float_shares = data.float_shares;
                        row.short_pct = data.short_pct;
                        row.catalyst = data.catalyst;
                        row.news_headlines = data.news_headlines;
                        row.avg_volume = data.avg_volume;
                        if let (Some(vol), Some(avg)) = (row.volume, data.avg_volume) {
                            if avg > 0 {
                                row.rvol = Some(vol as f64 / avg as f64);
                            }
                        }
                        row.enriched = true;
                    }

                    events.push(EngineEvent::EnrichComplete { symbol });
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
        if let Ok(client) = tws::TwsClient::connect(&self.settings.host, &ports, 0) {
            self.connected_port = Some(client.connected_port);
            client.disconnect();
        }
    }

    /// Load today's sightings from Supabase and populate alert state.
    /// Returns (loaded_count, needs_enrichment_count).
    pub fn init_from_sightings(&mut self, rt: &tokio::runtime::Handle) -> (usize, usize) {
        if let Some(ref db) = self.db {
            if let Ok(today) = rt.block_on(db.get_today()) {
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

                    let news_headlines: Vec<String> = s
                        .news_headlines
                        .as_deref()
                        .and_then(|h| serde_json::from_str(h).ok())
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
                        scanner_hits: n_scans,
                        news_headlines,
                        enriched: enrichment_fresh,
                        avg_volume: s.avg_volume,
                    });
                    if !enrichment_fresh {
                        needs_enrich += 1;
                        self.queue_enrich(&s.symbol, n_scans);
                    }
                }
                info!(loaded, needs_enrich, "sightings loaded");
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
                    info!(symbol = %req.symbol, priority = req.scanner_hits, "enriching via Yahoo");
                    rt_handle.block_on(fetch_enrichment(&client, &req.symbol))
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
            scanner_hits: 3,
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
