use anyhow::Result;
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, warn};

use crate::catalyst::classify_catalyst;
use crate::models::ScanResult;

/// Fetch Yahoo Finance data for a single symbol.
async fn fetch_yahoo_info(client: &Client, symbol: &str) -> Result<Value> {
    let url = format!(
        "https://query1.finance.yahoo.com/v10/finance/quoteSummary/{symbol}?modules=summaryProfile,defaultKeyStatistics,financialData,price"
    );
    let resp = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await?;
    let json: Value = resp.json().await?;
    Ok(json)
}

/// Fetch recent news for a symbol from Yahoo Finance.
async fn fetch_yahoo_news(client: &Client, symbol: &str) -> Result<Vec<Value>> {
    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/search?q={symbol}&newsCount=5&quotesCount=0"
    );
    let resp = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await?;
    let json: Value = resp.json().await?;
    let news = json
        .get("news")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(news)
}

/// Extract a nested field from Yahoo Finance quoteSummary response.
fn extract_raw(data: &Value, module: &str, field: &str) -> Option<Value> {
    data.pointer(&format!(
        "/quoteSummary/result/0/{module}/{field}/raw"
    ))
    .cloned()
}

fn extract_str(data: &Value, module: &str, field: &str) -> Option<String> {
    data.pointer(&format!(
        "/quoteSummary/result/0/{module}/{field}"
    ))
    .and_then(|v| v.as_str())
    .map(|s| s.to_string())
}

/// Enrichment data fetched from Yahoo Finance.
#[derive(Debug, Clone, Default)]
pub struct EnrichmentData {
    pub name: Option<String>,
    pub sector: Option<String>,
    pub industry: Option<String>,
    pub float_shares: Option<f64>,
    pub short_pct: Option<f64>,
    pub avg_volume: Option<i64>,
    pub catalyst: Option<String>,
}

/// Fetch enrichment data for a single symbol.
pub async fn fetch_enrichment(client: &Client, symbol: &str) -> EnrichmentData {
    let mut data = EnrichmentData::default();

    // Fetch info and news concurrently
    let (info_result, news_result) =
        tokio::join!(fetch_yahoo_info(client, symbol), fetch_yahoo_news(client, symbol));

    if let Ok(info) = info_result {
        data.name = extract_str(&info, "price", "shortName");
        data.sector = extract_str(&info, "summaryProfile", "sector");
        data.industry = extract_str(&info, "summaryProfile", "industry");
        data.float_shares = extract_raw(&info, "defaultKeyStatistics", "floatShares")
            .and_then(|v| v.as_f64());
        data.short_pct = extract_raw(&info, "defaultKeyStatistics", "shortPercentOfFloat")
            .and_then(|v| v.as_f64());
        data.avg_volume = extract_raw(&info, "price", "averageDailyVolume3Month")
            .and_then(|v| v.as_i64());
    } else if let Err(e) = info_result {
        warn!("Yahoo Finance info fetch failed for {symbol}: {e}");
    }

    if let Ok(news) = news_result {
        data.catalyst = classify_catalyst(&news);
    } else if let Err(e) = news_result {
        debug!("Yahoo Finance news fetch failed for {symbol}: {e}");
    }

    data
}

/// Enrich a list of scan results with Yahoo Finance data.
/// Runs enrichment concurrently for all symbols.
pub async fn enrich_results(results: &mut [ScanResult]) {
    let client = Client::new();
    let symbols: Vec<String> = results.iter().map(|r| r.symbol.clone()).collect();

    let mut handles = Vec::new();
    for symbol in &symbols {
        let client = client.clone();
        let symbol = symbol.clone();
        handles.push(tokio::spawn(async move {
            fetch_enrichment(&client, &symbol).await
        }));
    }

    for (i, handle) in handles.into_iter().enumerate() {
        if let Ok(data) = handle.await {
            let r = &mut results[i];
            r.name = data.name;
            r.sector = data.sector;
            r.industry = data.industry;
            r.float_shares = data.float_shares;
            r.short_pct = data.short_pct;
            r.avg_volume = data.avg_volume;
            r.catalyst = data.catalyst;
            // Calculate relative volume
            if let (Some(vol), Some(avg)) = (r.volume, data.avg_volume) {
                if avg > 0 {
                    r.rvol = Some(vol as f64 / avg as f64);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enrichment_data_default() {
        let d = EnrichmentData::default();
        assert!(d.name.is_none());
        assert!(d.sector.is_none());
        assert!(d.catalyst.is_none());
    }

    #[test]
    fn test_extract_raw_missing() {
        let data = serde_json::json!({});
        assert!(extract_raw(&data, "price", "shortName").is_none());
    }

    #[test]
    fn test_extract_str_present() {
        let data = serde_json::json!({
            "quoteSummary": {
                "result": [{
                    "price": {
                        "shortName": "Apple Inc."
                    }
                }]
            }
        });
        assert_eq!(extract_str(&data, "price", "shortName"), Some("Apple Inc.".to_string()));
    }
}
