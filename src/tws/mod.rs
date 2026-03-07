use std::collections::HashMap;

use anyhow::Result;
use tracing::{debug, info, warn};

use crate::models::{ScanResult, DEFAULT_PORTS};

/// Build an ibapi ScannerSubscription from our parameters.
fn build_subscription(
    scan_code: &str,
    rows: u32,
    min_price: Option<f64>,
    max_price: Option<f64>,
) -> ibapi::scanner::ScannerSubscription {
    ibapi::scanner::ScannerSubscription {
        number_of_rows: rows as i32,
        instrument: Some("STK".to_string()),
        location_code: Some("STK.US.MAJOR".to_string()),
        scan_code: Some(scan_code.to_string()),
        above_price: min_price,
        below_price: max_price,
        above_volume: Some(100_000),
        ..Default::default()
    }
}

/// Convert ibapi ScannerData to our ScanResult.
fn scanner_data_to_result(data: &ibapi::scanner::ScannerData) -> ScanResult {
    let c = &data.contract_details.contract;
    let name = if data.contract_details.long_name.is_empty() {
        None
    } else {
        Some(data.contract_details.long_name.clone())
    };
    ScanResult {
        rank: (data.rank + 1) as u32,
        symbol: c.symbol.to_string(),
        con_id: c.contract_id as i64,
        exchange: if c.primary_exchange.to_string().is_empty() {
            "SMART".to_string()
        } else {
            c.primary_exchange.to_string()
        },
        currency: c.currency.to_string(),
        name,
        ..Default::default()
    }
}

/// Try connecting to TWS on the given ports, return the first successful client and port.
async fn connect(
    host: &str,
    ports: &[u16],
    client_id: i32,
) -> Result<(ibapi::Client, u16)> {
    let ports = if ports.is_empty() { DEFAULT_PORTS } else { ports };

    for &port in ports {
        let addr = format!("{host}:{port}");
        match ibapi::Client::connect(&addr, client_id).await {
            Ok(client) => {
                info!("Connected to TWS on port {port}");
                eprintln!("Connected to TWS on port {port}");
                return Ok((client, port));
            }
            Err(e) => {
                debug!("Connection failed on port {port}: {e}");
                continue;
            }
        }
    }

    anyhow::bail!(
        "Could not connect on any port: {}",
        ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")
    )
}

/// Run a scanner subscription and return results with the connected port.
pub async fn run_scan(
    scanner_code: &str,
    host: &str,
    ports: &[u16],
    client_id: i32,
    rows: u32,
    min_price: Option<f64>,
    max_price: Option<f64>,
) -> (Vec<ScanResult>, Option<u16>) {
    eprintln!("Scanning {scanner_code} (rows={rows})...");

    let (client, port) = match connect(host, ports, client_id).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return (vec![], None);
        }
    };

    let sub = build_subscription(scanner_code, rows, min_price, max_price);
    let mut subscription = match client.scanner_subscription(&sub, &vec![]).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to request scanner: {e}");
            return (vec![], Some(port));
        }
    };

    let results = match subscription.next().await {
        Some(Ok(data)) => {
            let count = data.len();
            info!(scanner_code, count, "scanner results received");
            data.iter().map(|d| scanner_data_to_result(d)).collect()
        }
        Some(Err(e)) => {
            eprintln!("Scanner error: {e}");
            vec![]
        }
        None => vec![],
    };

    subscription.cancel().await;
    (results, Some(port))
}

/// Run multiple scanner subscriptions over a single TWS connection.
/// Returns (symbol_scanners, symbol_data, connected_port).
pub async fn run_poll_scan(
    scanners: &[(&str, i32)],
    host: &str,
    ports: &[u16],
    _client_id: i32,
    rows: u32,
    min_price: Option<f64>,
    max_price: Option<f64>,
) -> (HashMap<String, Vec<String>>, HashMap<String, ScanResult>, Option<u16>) {
    // Use client_id 10 for the shared connection
    let (client, port) = match connect(host, ports, 10).await {
        Ok(c) => c,
        Err(e) => {
            warn!("Poll scan connect failed: {e}");
            return (HashMap::new(), HashMap::new(), None);
        }
    };

    let mut symbol_scanners: HashMap<String, Vec<String>> = HashMap::new();
    let mut symbol_data: HashMap<String, ScanResult> = HashMap::new();

    for (i, &(code, _cid)) in scanners.iter().enumerate() {
        let sub = build_subscription(code, rows, min_price, max_price);
        let mut subscription = match client.scanner_subscription(&sub, &vec![]).await {
            Ok(s) => s,
            Err(e) => {
                warn!(code, "failed to subscribe scanner: {e}");
                continue;
            }
        };

        let results: Vec<ScanResult> = match subscription.next().await {
            Some(Ok(data)) => data.iter().map(|d| scanner_data_to_result(d)).collect(),
            Some(Err(e)) => {
                warn!(code, "scanner error: {e}");
                vec![]
            }
            None => vec![],
        };

        subscription.cancel().await;

        let count = results.len();
        info!(scanner = i + 1, total = scanners.len(), code, count, "poll scanner results");

        for r in results {
            let sym = r.symbol.clone();
            symbol_scanners
                .entry(sym.clone())
                .or_default()
                .push(code.to_string());
            symbol_data.entry(sym).or_insert(r);
        }
    }

    (symbol_scanners, symbol_data, Some(port))
}

/// Fetch scanner parameters XML from TWS.
pub async fn fetch_scanner_params(host: &str, ports: &[u16], client_id: i32) -> Option<String> {
    let (client, _port) = connect(host, ports, client_id).await.ok()?;

    match client.scanner_parameters().await {
        Ok(xml) => Some(xml),
        Err(e) => {
            eprintln!("Failed to get scanner parameters: {e}");
            None
        }
    }
}

/// Probe TWS to find the first connectable port.
pub async fn probe_port(host: &str, ports: &[u16]) -> Option<u16> {
    let (_client, port) = connect(host, ports, 0).await.ok()?;
    Some(port)
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
