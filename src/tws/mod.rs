pub mod messages;

use std::collections::HashMap;
use std::io::{BufReader, BufWriter, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use tracing::{debug, error, info, warn};

use crate::models::{ScanResult, DEFAULT_PORTS};
use messages::*;

/// State shared between the reader thread and main thread.
#[derive(Debug, Default)]
struct TwsState {
    connected: bool,
    #[allow(dead_code)]
    server_version: Option<i32>,
    results: HashMap<i32, ScanResult>,
    contracts: HashMap<i32, (i64, String, String)>, // req_id -> (conId, symbol, currency)
    scanner_done: bool,
    scanner_params_xml: Option<String>,
    scanner_params_done: bool,
    next_req_id: i32,
}

/// TWS client that connects to Interactive Brokers TWS/IB Gateway.
pub struct TwsClient {
    writer: BufWriter<TcpStream>,
    state: Arc<Mutex<TwsState>>,
    _reader_handle: std::thread::JoinHandle<()>,
}

impl TwsClient {
    /// Connect to TWS, trying ports in order. Returns connected client.
    pub fn connect(host: &str, ports: &[u16], client_id: i32) -> Result<Self> {
        let ports = if ports.is_empty() { DEFAULT_PORTS } else { ports };

        for &port in ports {
            // Quick TCP check
            match TcpStream::connect_timeout(
                &format!("{host}:{port}").parse().unwrap(),
                Duration::from_secs(2),
            ) {
                Ok(stream) => {
                    match Self::handshake(stream, client_id) {
                        Ok(client) => {
                            info!("Connected to TWS on port {port}");
                            println!("Connected to TWS on port {port}");
                            return Ok(client);
                        }
                        Err(e) => {
                            debug!("Handshake failed on port {port}: {e}");
                            continue;
                        }
                    }
                }
                Err(_) => continue,
            }
        }

        anyhow::bail!(
            "Could not connect on any port: {}",
            ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")
        )
    }

    fn handshake(stream: TcpStream, client_id: i32) -> Result<Self> {
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let read_stream = stream.try_clone()?;

        let mut writer = BufWriter::new(stream);

        // Send handshake
        let handshake = build_handshake();
        writer.write_all(&handshake)?;
        writer.flush()?;

        // Read server version response (not length-prefixed, just raw text until \0)
        let mut reader = BufReader::new(read_stream);
        let mut byte = [0u8; 1];
        let mut version_str = String::new();
        loop {
            reader.read_exact(&mut byte)?;
            if byte[0] == 0 {
                break;
            }
            version_str.push(byte[0] as char);
        }
        let server_version: i32 = version_str.trim().parse().unwrap_or(0);
        debug!("Server version: {server_version}");

        // Read server time (until \0)
        let mut time_str = String::new();
        loop {
            reader.read_exact(&mut byte)?;
            if byte[0] == 0 {
                break;
            }
            time_str.push(byte[0] as char);
        }
        debug!("Server time: {time_str}");

        // Send START_API
        let start_msg = build_start_api(client_id);
        writer.write_all(&start_msg)?;
        writer.flush()?;

        let state = Arc::new(Mutex::new(TwsState {
            server_version: Some(server_version),
            next_req_id: 1000,
            ..Default::default()
        }));

        // Start reader thread
        let state_clone = Arc::clone(&state);
        let reader_handle = std::thread::spawn(move || {
            Self::reader_loop(reader, state_clone);
        });

        // Wait for nextValidId (connection confirmation)
        let start = std::time::Instant::now();
        loop {
            if start.elapsed() > Duration::from_secs(10) {
                anyhow::bail!("Timeout waiting for connection confirmation");
            }
            {
                let s = state.lock().unwrap();
                if s.connected {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        Ok(Self {
            writer,
            state,
            _reader_handle: reader_handle,
        })
    }

    fn reader_loop(mut reader: BufReader<TcpStream>, state: Arc<Mutex<TwsState>>) {
        loop {
            match read_message(&mut reader) {
                Ok(fields) => {
                    if fields.is_empty() {
                        continue;
                    }
                    Self::process_message(&fields, &state);
                }
                Err(e) => {
                    debug!("Reader loop ended: {e}");
                    break;
                }
            }
        }
    }

    fn process_message(fields: &[String], state: &Arc<Mutex<TwsState>>) {
        if fields.is_empty() {
            return;
        }
        let msg_type = &fields[0];

        match msg_type.as_str() {
            in_msg::NEXT_VALID_ID => {
                let mut s = state.lock().unwrap();
                s.connected = true;
                debug!("Received nextValidId — connected");
            }
            in_msg::ERR_MSG => {
                // fields: [msg_type, version, req_id, error_code, error_string, ...]
                if fields.len() >= 5 {
                    let error_code: i32 = fields[3].parse().unwrap_or(-1);
                    let error_string = &fields[4];

                    if error_code == 502 {
                        error!("Cannot connect to TWS. Make sure TWS/IB Gateway is running.");
                    } else if !NONFATAL_ERRORS.contains(&error_code) {
                        let req_id: i32 = fields[2].parse().unwrap_or(-1);
                        let s = state.lock().unwrap();
                        if let Some(result) = s.results.get(&req_id) {
                            warn!("{}: Error {} - {}", result.symbol, error_code, error_string);
                        } else {
                            warn!("Error {}: {}", error_code, error_string);
                        }
                    }
                }
            }
            in_msg::SCANNER_DATA => {
                Self::handle_scanner_data(fields, state);
            }
            in_msg::SCANNER_PARAMETERS => {
                // fields: [msg_type, version, xml]
                if fields.len() >= 3 {
                    let mut s = state.lock().unwrap();
                    s.scanner_params_xml = Some(fields[2].clone());
                    s.scanner_params_done = true;
                }
            }
            in_msg::TICK_PRICE => {
                Self::handle_tick_price(fields, state);
            }
            in_msg::TICK_SIZE => {
                Self::handle_tick_size(fields, state);
            }
            _ => {
                // Ignore unknown message types
            }
        }
    }

    fn handle_scanner_data(fields: &[String], state: &Arc<Mutex<TwsState>>) {
        // Scanner data format (v3):
        // [msg_type, version, req_id, num_elements, then for each:
        //  rank, conId, symbol, secType, lastTradeDateOrContractMonth,
        //  strike, right, exchange, currency, localSymbol, marketName,
        //  tradingClass, distance, benchmark, projection, legsStr]
        if fields.len() < 4 {
            return;
        }
        let version: i32 = fields[1].parse().unwrap_or(0);
        let _req_id: i32 = fields[2].parse().unwrap_or(0);
        let num_elements: i32 = fields[3].parse().unwrap_or(0);

        if num_elements < 0 {
            // scannerDataEnd signal
            let mut s = state.lock().unwrap();
            let count = s.results.len();
            println!("Found {count} stocks, fetching market data...\n");
            s.scanner_done = true;
            return;
        }

        let mut idx = 4;
        let mut s = state.lock().unwrap();
        for _ in 0..num_elements {
            if idx + 15 >= fields.len() {
                break;
            }
            let rank: u32 = fields[idx].parse().unwrap_or(0);
            let con_id: i64 = fields[idx + 1].parse().unwrap_or(0);
            let symbol = fields[idx + 2].clone();
            let _sec_type = &fields[idx + 3];
            // Skip several fields to get to exchange and currency
            let exchange = if idx + 7 < fields.len() {
                fields[idx + 7].clone()
            } else {
                "SMART".to_string()
            };
            let currency = if idx + 8 < fields.len() {
                fields[idx + 8].clone()
            } else {
                "USD".to_string()
            };

            let mkt_req_id = s.next_req_id + rank as i32;
            s.results.insert(
                mkt_req_id,
                ScanResult {
                    rank: rank + 1,
                    symbol: symbol.clone(),
                    con_id,
                    exchange: if exchange.is_empty() {
                        "SMART".to_string()
                    } else {
                        exchange.clone()
                    },
                    currency: currency.clone(),
                    ..Default::default()
                },
            );
            s.contracts
                .insert(mkt_req_id, (con_id, symbol, currency));

            // Each scanner result has 16 fields (for v3)
            idx += if version >= 3 { 16 } else { 14 };
        }
    }

    fn handle_tick_price(fields: &[String], state: &Arc<Mutex<TwsState>>) {
        // fields: [msg_type, version, req_id, tick_type, price, ...]
        if fields.len() < 5 {
            return;
        }
        let req_id: i32 = fields[2].parse().unwrap_or(-1);
        let tick_type_id: i32 = fields[3].parse().unwrap_or(-1);
        let price: f64 = fields[4].parse().unwrap_or(0.0);

        if price <= 0.0 {
            return;
        }

        let mut s = state.lock().unwrap();
        if let Some(r) = s.results.get_mut(&req_id) {
            match tick_type_id {
                tick_type::BID | tick_type::DELAYED_BID => r.bid = Some(price),
                tick_type::ASK | tick_type::DELAYED_ASK => r.ask = Some(price),
                tick_type::LAST | tick_type::DELAYED_LAST => {
                    r.last = Some(price);
                    if let Some(close) = r.close {
                        if close > 0.0 {
                            r.change = Some(price - close);
                            r.change_pct = Some(((price - close) / close) * 100.0);
                        }
                    }
                }
                tick_type::CLOSE | tick_type::DELAYED_CLOSE => {
                    r.close = Some(price);
                    if let Some(last) = r.last {
                        if price > 0.0 {
                            r.change = Some(last - price);
                            r.change_pct = Some(((last - price) / price) * 100.0);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_tick_size(fields: &[String], state: &Arc<Mutex<TwsState>>) {
        // fields: [msg_type, version, req_id, tick_type, size]
        if fields.len() < 5 {
            return;
        }
        let req_id: i32 = fields[2].parse().unwrap_or(-1);
        let tick_type_id: i32 = fields[3].parse().unwrap_or(-1);
        let size: i64 = fields[4].parse().unwrap_or(0);

        if tick_type_id == tick_type::VOLUME {
            let mut s = state.lock().unwrap();
            if let Some(r) = s.results.get_mut(&req_id) {
                r.volume = Some(size);
            }
        }
    }

    /// Request market data type (e.g., 4 for delayed frozen).
    pub fn req_market_data_type(&mut self, data_type: i32) -> Result<()> {
        write_message(
            &mut self.writer,
            &[out_msg::REQ_MKT_DATA_TYPE, "1", &data_type.to_string()],
        )?;
        Ok(())
    }

    /// Request a scanner subscription.
    pub fn req_scanner_subscription(
        &mut self,
        req_id: i32,
        scan_code: &str,
        rows: u32,
        min_price: Option<f64>,
        max_price: Option<f64>,
    ) -> Result<()> {
        let rows_str = rows.to_string();
        let req_id_str = req_id.to_string();

        // Build scanner subscription message
        let fields: Vec<String> = vec![
            out_msg::REQ_SCANNER_SUBSCRIPTION.to_string(),
            "4".to_string(), // version
            req_id_str,
            rows_str,        // numberOfRows
            "STK".to_string(), // instrument
            "STK.US.MAJOR".to_string(), // locationCode
            scan_code.to_string(), // scanCode
        ];

        // Add filter tag-value pairs as additional fields
        // Above/below price, volume filters
        // This is a simplified version - the actual protocol is more complex
        // with specific field positions for the scanner subscription

        // For now, build the complete message with all required fields
        let mut payload = Vec::new();
        payload.extend_from_slice(out_msg::REQ_SCANNER_SUBSCRIPTION.as_bytes());
        payload.push(0);
        payload.extend_from_slice(b"4"); // version
        payload.push(0);
        payload.extend_from_slice(req_id.to_string().as_bytes());
        payload.push(0);
        payload.extend_from_slice(rows.to_string().as_bytes()); // numberOfRows
        payload.push(0);
        payload.extend_from_slice(b"STK"); // instrument
        payload.push(0);
        payload.extend_from_slice(b"STK.US.MAJOR"); // locationCode
        payload.push(0);
        payload.extend_from_slice(scan_code.as_bytes()); // scanCode
        payload.push(0);

        // Above/below market price (empty = no filter for these specific fields)
        payload.push(0); // abovePrice
        payload.push(0); // belowPrice
        payload.push(0); // aboveVolume
        payload.push(0); // marketCapAbove
        payload.push(0); // marketCapBelow
        payload.push(0); // moodyRatingAbove
        payload.push(0); // moodyRatingBelow
        payload.push(0); // spRatingAbove
        payload.push(0); // spRatingBelow
        payload.push(0); // maturityDateAbove
        payload.push(0); // maturityDateBelow
        payload.push(0); // couponRateAbove
        payload.push(0); // couponRateBelow
        payload.push(0); // excludeConvertible
        payload.push(0); // averageOptionVolumeAbove (v4+)
        payload.push(0); // scannerSettingPairs (v4+)
        payload.push(0); // stockTypeFilter (v4+)

        // Scanner subscription filter options (tag-value list)
        let mut filter_count = 1; // volume filter always
        if min_price.is_some() {
            filter_count += 1;
        }
        if max_price.is_some() {
            filter_count += 1;
        }
        payload.extend_from_slice(filter_count.to_string().as_bytes());
        payload.push(0);

        if let Some(min_p) = min_price {
            payload.extend_from_slice(b"priceAbove");
            payload.push(0);
            payload.extend_from_slice(format!("{min_p}").as_bytes());
            payload.push(0);
        }
        if let Some(max_p) = max_price {
            payload.extend_from_slice(b"priceBelow");
            payload.push(0);
            payload.extend_from_slice(format!("{max_p}").as_bytes());
            payload.push(0);
        }
        payload.extend_from_slice(b"volumeAbove");
        payload.push(0);
        payload.extend_from_slice(b"100000");
        payload.push(0);

        // No scanner subscription options
        payload.extend_from_slice(b"0");
        payload.push(0);

        let len = payload.len() as u32;
        self.writer.write_all(&len.to_be_bytes())?;
        self.writer.write_all(&payload)?;
        self.writer.flush()?;

        drop(fields); // Suppress unused warning

        Ok(())
    }

    /// Cancel a scanner subscription.
    pub fn cancel_scanner_subscription(&mut self, req_id: i32) -> Result<()> {
        write_message(
            &mut self.writer,
            &[
                out_msg::CANCEL_SCANNER_SUBSCRIPTION,
                "1",
                &req_id.to_string(),
            ],
        )?;
        Ok(())
    }

    /// Request scanner parameters XML.
    pub fn req_scanner_parameters(&mut self) -> Result<()> {
        write_message(&mut self.writer, &[out_msg::REQ_SCANNER_PARAMETERS, "1"])?;
        Ok(())
    }

    /// Request market data for all scanner results.
    pub fn request_market_data(&mut self) -> Result<()> {
        let contracts: Vec<(i32, i64, String, String)> = {
            let s = self.state.lock().unwrap();
            s.contracts
                .iter()
                .map(|(&req_id, (con_id, symbol, currency))| {
                    (req_id, *con_id, symbol.clone(), currency.clone())
                })
                .collect()
        };

        for (req_id, con_id, symbol, currency) in contracts {
            let mut payload = Vec::new();
            payload.extend_from_slice(out_msg::REQ_MKT_DATA.as_bytes());
            payload.push(0);
            payload.extend_from_slice(b"11"); // version
            payload.push(0);
            payload.extend_from_slice(req_id.to_string().as_bytes());
            payload.push(0);
            payload.extend_from_slice(con_id.to_string().as_bytes()); // conId
            payload.push(0);
            payload.extend_from_slice(symbol.as_bytes()); // symbol
            payload.push(0);
            payload.extend_from_slice(b"STK"); // secType
            payload.push(0);
            payload.push(0); // lastTradeDateOrContractMonth
            payload.push(0); // strike
            payload.push(0); // right
            payload.push(0); // multiplier
            payload.extend_from_slice(b"SMART"); // exchange
            payload.push(0);
            payload.push(0); // primaryExch
            payload.extend_from_slice(currency.as_bytes()); // currency
            payload.push(0);
            payload.push(0); // localSymbol
            payload.push(0); // tradingClass
            payload.push(0); // genericTickList
            payload.extend_from_slice(b"0"); // snapshot
            payload.push(0);
            payload.extend_from_slice(b"0"); // regulatorySnapshot
            payload.push(0);
            payload.push(0); // mktDataOptions tag count = 0
            payload.push(0);

            let len = payload.len() as u32;
            self.writer.write_all(&len.to_be_bytes())?;
            self.writer.write_all(&payload)?;
        }
        self.writer.flush()?;
        Ok(())
    }

    /// Cancel market data for all contracts.
    pub fn cancel_market_data(&mut self) -> Result<()> {
        let req_ids: Vec<i32> = {
            let s = self.state.lock().unwrap();
            s.contracts.keys().copied().collect()
        };
        for req_id in req_ids {
            write_message(
                &mut self.writer,
                &[out_msg::CANCEL_MKT_DATA, "2", &req_id.to_string()],
            )?;
        }
        Ok(())
    }

    /// Wait for scanner to complete, returns true if data received.
    pub fn wait_scanner_done(&self, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        loop {
            if start.elapsed() > timeout {
                return false;
            }
            {
                let s = self.state.lock().unwrap();
                if s.scanner_done {
                    return true;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Wait for scanner parameters to be received.
    pub fn wait_scanner_params(&self, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        loop {
            if start.elapsed() > timeout {
                return false;
            }
            {
                let s = self.state.lock().unwrap();
                if s.scanner_params_done {
                    return true;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Get scanner parameters XML.
    pub fn get_scanner_params_xml(&self) -> Option<String> {
        let s = self.state.lock().unwrap();
        s.scanner_params_xml.clone()
    }

    /// Get results sorted by rank.
    pub fn get_results(&self) -> Vec<ScanResult> {
        let s = self.state.lock().unwrap();
        let mut results: Vec<ScanResult> = s.results.values().cloned().collect();
        results.sort_by_key(|r| r.rank);
        results
    }

    /// Disconnect from TWS.
    pub fn disconnect(self) {
        // Writer goes out of scope, closing the connection.
        // Reader thread will detect the closed connection and exit.
        drop(self.writer);
    }
}

/// Run a scanner subscription and return enriched results.
pub fn run_scan(
    scanner_code: &str,
    host: &str,
    ports: &[u16],
    client_id: i32,
    rows: u32,
    min_price: Option<f64>,
    max_price: Option<f64>,
) -> Vec<ScanResult> {
    println!("\nScanning {scanner_code} (rows={rows})...\n");

    let mut client = match TwsClient::connect(host, ports, client_id) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return vec![];
        }
    };

    // Request delayed frozen data
    if let Err(e) = client.req_market_data_type(4) {
        eprintln!("Failed to set market data type: {e}");
    }

    // Request scanner subscription
    if let Err(e) = client.req_scanner_subscription(1, scanner_code, rows, min_price, max_price) {
        eprintln!("Failed to request scanner: {e}");
        client.disconnect();
        return vec![];
    }

    // Wait for scanner results
    if !client.wait_scanner_done(Duration::from_secs(30)) {
        eprintln!("Timeout waiting for scanner results");
        client.disconnect();
        return vec![];
    }

    // Request market data for all results
    println!("Waiting for market data...");
    if let Err(e) = client.request_market_data() {
        eprintln!("Failed to request market data: {e}");
    }
    std::thread::sleep(Duration::from_secs(5));

    // Cancel market data
    let _ = client.cancel_market_data();
    std::thread::sleep(Duration::from_millis(500));

    let results = client.get_results();
    client.disconnect();
    results
}

/// Fetch scanner parameters XML from TWS.
pub fn fetch_scanner_params(host: &str, ports: &[u16], client_id: i32) -> Option<String> {
    let mut client = TwsClient::connect(host, ports, client_id).ok()?;

    if client.req_scanner_parameters().is_err() {
        client.disconnect();
        return None;
    }

    if !client.wait_scanner_params(Duration::from_secs(15)) {
        eprintln!("Timeout waiting for scanner parameters");
        client.disconnect();
        return None;
    }

    let xml = client.get_scanner_params_xml();
    client.disconnect();
    xml
}

/// Parse scanner parameters XML and group by instrument -> category.
/// Returns {instrument: {category: [(code, display_name)]}}
pub fn group_scans(
    xml: &str,
) -> HashMap<String, HashMap<String, Vec<(String, String)>>> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut tree: HashMap<String, HashMap<String, Vec<(String, String)>>> = HashMap::new();
    let mut reader = Reader::from_str(xml);

    let mut in_scan_type = false;
    let mut current_field = String::new();
    let mut code = String::new();
    let mut display_name = String::new();
    let mut vendor = String::new();
    let mut instruments = String::new();

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag == "ScanType" {
                    in_scan_type = true;
                    code.clear();
                    display_name.clear();
                    vendor.clear();
                    instruments.clear();
                } else if in_scan_type {
                    current_field = tag;
                }
            }
            Ok(Event::Text(e)) => {
                if in_scan_type {
                    let text = e.unescape().unwrap_or_default().to_string();
                    match current_field.as_str() {
                        "scanCode" => code = text,
                        "displayName" => display_name = text,
                        "vendor" => vendor = text,
                        "instruments" => instruments = text,
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag == "ScanType" && in_scan_type {
                    let (instrument, category) =
                        categorize_scan(&code, &display_name, &vendor, &instruments);
                    tree.entry(instrument)
                        .or_default()
                        .entry(category)
                        .or_default()
                        .push((code.clone(), display_name.clone()));
                    in_scan_type = false;
                }
                current_field.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                warn!("XML parse error: {e}");
                break;
            }
            _ => {}
        }
        buf.clear();
    }

    tree
}

/// Categorize a scanner into (instrument_type, category).
fn categorize_scan(code: &str, name: &str, vendor: &str, instruments: &str) -> (String, String) {
    // Vendor-based
    match vendor {
        "ALV" => return ("ETFs".to_string(), "ETF Scanners".to_string()),
        "REUTFUND" => return ("Funds".to_string(), "Analyst & Ratings".to_string()),
        "RCG" => return ("Stocks".to_string(), "Technicals (Recognia)".to_string()),
        "MSOWN" => return ("Stocks".to_string(), "Ownership".to_string()),
        "WSH" => return ("Stocks".to_string(), "Events".to_string()),
        "MOODY" | "SP" => return ("Bonds".to_string(), "Bond Ratings".to_string()),
        _ => {}
    }

    // Instrument-based
    if instruments.contains("BOND") && !instruments.contains("STK") {
        return ("Bonds".to_string(), "Bond Scanners".to_string());
    }
    if instruments.contains("FUND") && !instruments.contains("STK") {
        return ("Funds".to_string(), "Fund Scanners".to_string());
    }
    if instruments.contains("NATCOMB") {
        return ("Futures & Combos".to_string(), "Combos".to_string());
    }
    if instruments.contains("SLB") && !instruments.contains("STK") {
        return ("Stocks".to_string(), "Stock Borrow/Loan".to_string());
    }

    let name_l = name.to_lowercase();
    let code_l = code.to_lowercase();

    // Options
    if ["opt", "imp vol"].iter().any(|w| name_l.contains(w)) {
        return ("Options".to_string(), "Options Activity".to_string());
    }
    if name_l.contains("iv") || name_l.contains("hv") {
        return ("Options".to_string(), "Volatility Rank".to_string());
    }

    // Stock subcategories
    if ["gap", "open_perc", "after_hours"]
        .iter()
        .any(|w| code_l.contains(w))
    {
        return (
            "Stocks".to_string(),
            "Gaps & Extended Hours".to_string(),
        );
    }
    if ["perc_gain", "perc_lose"]
        .iter()
        .any(|w| code_l.contains(w))
    {
        return (
            "Stocks".to_string(),
            "Momentum & Gainers".to_string(),
        );
    }
    if ["volume", "active", "hot", "trade count", "trade rate"]
        .iter()
        .any(|w| name_l.contains(w))
    {
        return ("Stocks".to_string(), "Volume & Activity".to_string());
    }
    if (name_l.contains("high") || name_l.contains("low")) && code_l.contains("w_hl") {
        return ("Stocks".to_string(), "Highs & Lows".to_string());
    }
    if ["halted", "limit up", "not yet traded", "ipo"]
        .iter()
        .any(|w| name_l.contains(w))
    {
        return ("Stocks".to_string(), "Special".to_string());
    }
    if ["social", "sentiment", "tweet"]
        .iter()
        .any(|w| name_l.contains(w))
    {
        return ("Stocks".to_string(), "Social Sentiment".to_string());
    }
    if ["shortable", "fee rate", "utilization"]
        .iter()
        .any(|w| name_l.contains(w))
    {
        return ("Stocks".to_string(), "Short Interest".to_string());
    }
    if name_l.contains("shares outstanding") {
        return ("Stocks".to_string(), "Fundamentals".to_string());
    }
    if ["dividend", "yield"].iter().any(|w| name_l.contains(w)) {
        return ("Stocks".to_string(), "Dividends".to_string());
    }
    if ["ema", "macd", "ppo", "price vs"]
        .iter()
        .any(|w| name_l.contains(w))
    {
        return (
            "Stocks".to_string(),
            "Technical Indicators".to_string(),
        );
    }

    ("Stocks".to_string(), "Other".to_string())
}

/// Print scanner parameters in a formatted table.
pub fn print_scanner_params(xml: &str, scan_group: Option<&str>) {
    let tree = group_scans(xml);
    let total: usize = tree.values().flat_map(|cats| cats.values().map(|s| s.len())).sum();

    if let Some(query) = scan_group {
        let query_lower = query.to_lowercase();
        for inst in tree.keys() {
            for (cat, entries) in &tree[inst] {
                if cat.to_lowercase().contains(&query_lower) {
                    println!("{inst} > {cat} ({} scanners)", entries.len());
                    println!("{:<30}  {}", "Scanner Code", "Description");
                    println!("{}", "-".repeat(60));
                    let mut sorted = entries.clone();
                    sorted.sort_by(|a, b| a.1.cmp(&b.1));
                    for (code, disp) in &sorted {
                        println!("{code:<30}  {disp}");
                    }
                    return;
                }
            }
        }
        println!("No group matching '{query}'");
        return;
    }

    println!("Scanners -- {total} total");
    println!("{:<20}  {:<30}  {:>5}", "Instrument", "Category", "Count");
    println!("{}", "-".repeat(60));
    let mut instruments: Vec<_> = tree.keys().collect();
    instruments.sort();
    for inst in instruments {
        let cats = &tree[inst];
        let mut cat_names: Vec<_> = cats.keys().collect();
        cat_names.sort();
        let mut first = true;
        for cat in cat_names {
            let count = cats[cat].len();
            let inst_col = if first { inst.as_str() } else { "" };
            println!("{inst_col:<20}  {cat:<30}  {count:>5}");
            first = false;
        }
    }
    println!("\nUse 'list <group>' to expand a category.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_categorize_scan_vendor() {
        assert_eq!(
            categorize_scan("X", "Y", "ALV", ""),
            ("ETFs".to_string(), "ETF Scanners".to_string())
        );
        assert_eq!(
            categorize_scan("X", "Y", "MOODY", ""),
            ("Bonds".to_string(), "Bond Ratings".to_string())
        );
    }

    #[test]
    fn test_categorize_scan_instrument() {
        assert_eq!(
            categorize_scan("X", "Y", "", "BOND"),
            ("Bonds".to_string(), "Bond Scanners".to_string())
        );
        assert_eq!(
            categorize_scan("X", "Y", "", "NATCOMB"),
            ("Futures & Combos".to_string(), "Combos".to_string())
        );
    }

    #[test]
    fn test_categorize_scan_stock_subcategories() {
        assert_eq!(
            categorize_scan("HIGH_OPEN_GAP", "Gap Up", "", "STK"),
            ("Stocks".to_string(), "Gaps & Extended Hours".to_string())
        );
        assert_eq!(
            categorize_scan("TOP_PERC_GAIN", "Top % Gainers", "", "STK"),
            ("Stocks".to_string(), "Momentum & Gainers".to_string())
        );
        assert_eq!(
            categorize_scan("HOT_BY_VOLUME", "Hot by Volume", "", "STK"),
            ("Stocks".to_string(), "Volume & Activity".to_string())
        );
    }

    #[test]
    fn test_categorize_scan_default() {
        assert_eq!(
            categorize_scan("UNKNOWN", "Unknown Scanner", "", "STK"),
            ("Stocks".to_string(), "Other".to_string())
        );
    }

    #[test]
    fn test_group_scans_simple_xml() {
        let xml = r#"<?xml version="1.0"?>
        <ScanParameterResponse>
            <ScanTypeList>
                <ScanType>
                    <scanCode>TOP_PERC_GAIN</scanCode>
                    <displayName>Top % Gainers</displayName>
                    <vendor></vendor>
                    <instruments>STK</instruments>
                </ScanType>
                <ScanType>
                    <scanCode>HOT_BY_VOLUME</scanCode>
                    <displayName>Hot by Volume</displayName>
                    <vendor></vendor>
                    <instruments>STK</instruments>
                </ScanType>
            </ScanTypeList>
        </ScanParameterResponse>"#;

        let tree = group_scans(xml);
        assert!(tree.contains_key("Stocks"));
        let stocks = &tree["Stocks"];
        // Both should be categorized under some stock subcategory
        let total: usize = stocks.values().map(|v| v.len()).sum();
        assert_eq!(total, 2);
    }
}
