use anyhow::Result;
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, warn};

use crate::catalyst::classify_catalyst;
use crate::models::{NewsHeadline, ScanResult};

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// Simple percent-encoding for URL query values.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

/// Yahoo Finance auth credentials (cookie + crumb).
#[derive(Debug, Clone)]
pub struct YahooAuth {
    pub cookie: String,
    pub crumb: String,
}

/// Fetch Yahoo Finance auth (cookie + crumb) required for API access.
pub async fn fetch_yahoo_auth(client: &Client) -> Result<YahooAuth> {
    // Step 1: Hit fc.yahoo.com to get set-cookie
    let resp = client
        .get("https://fc.yahoo.com")
        .header("User-Agent", USER_AGENT)
        .send()
        .await?;

    let cookies: String = resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|c| c.split(';').next().unwrap_or("").trim().to_string())
        .collect::<Vec<_>>()
        .join("; ");

    if cookies.is_empty() {
        anyhow::bail!("No cookies from fc.yahoo.com");
    }

    // Step 2: Fetch crumb using cookies
    let crumb_resp = client
        .get("https://query2.finance.yahoo.com/v1/test/getcrumb")
        .header("User-Agent", USER_AGENT)
        .header("Cookie", &cookies)
        .send()
        .await?
        .error_for_status()?;

    let crumb = crumb_resp.text().await?;
    if crumb.is_empty() || crumb.contains("Too Many Requests") {
        anyhow::bail!("Failed to get crumb: {crumb}");
    }

    Ok(YahooAuth { cookie: cookies, crumb })
}

/// Fetch Yahoo Finance data for a single symbol (with auth).
async fn fetch_yahoo_info(client: &Client, symbol: &str, auth: &YahooAuth) -> Result<Value> {
    let url = format!(
        "https://query2.finance.yahoo.com/v10/finance/quoteSummary/{}?modules=summaryProfile,defaultKeyStatistics,financialData,price&crumb={}",
        symbol,
        percent_encode(&auth.crumb)
    );
    let resp = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Cookie", &auth.cookie)
        .send()
        .await?
        .error_for_status()?;
    let json: Value = resp.json().await?;
    Ok(json)
}

/// Fetch recent news for a symbol from Yahoo Finance (with auth).
async fn fetch_yahoo_news(client: &Client, symbol: &str, auth: &YahooAuth) -> Result<Vec<Value>> {
    let url = format!(
        "https://query2.finance.yahoo.com/v8/finance/search?q={}&newsCount=5&quotesCount=0&crumb={}",
        symbol,
        percent_encode(&auth.crumb)
    );
    let resp = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Cookie", &auth.cookie)
        .send()
        .await?
        .error_for_status()?;
    let json: Value = resp.json().await?;
    let news = json
        .get("news")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(news)
}

/// Fetch recent news via Yahoo Finance RSS feed (no auth required, more reliable).
async fn fetch_yahoo_news_rss(client: &Client, symbol: &str) -> Result<Vec<Value>> {
    let url = format!(
        "https://feeds.finance.yahoo.com/rss/2.0/headline?s={}&region=US&lang=en-US",
        symbol
    );
    let resp = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .error_for_status()?;
    let body = resp.text().await?;

    let mut reader = quick_xml::Reader::from_str(&body);
    let mut items = Vec::new();
    let mut in_item = false;
    let mut in_title = false;
    let mut in_pub_date = false;
    let mut current_title = String::new();
    let mut current_pub_date = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => match e.name().as_ref() {
                b"item" => {
                    in_item = true;
                    current_title.clear();
                    current_pub_date.clear();
                }
                b"title" if in_item => in_title = true,
                b"pubDate" if in_item => in_pub_date = true,
                _ => {}
            },
            Ok(quick_xml::events::Event::End(e)) => match e.name().as_ref() {
                b"item" => {
                    if in_item && !current_title.is_empty() {
                        let pub_time = chrono::DateTime::parse_from_rfc2822(&current_pub_date)
                            .ok()
                            .map(|dt| dt.timestamp());
                        let mut item = serde_json::json!({"title": &current_title});
                        if let Some(t) = pub_time {
                            item["providerPublishTime"] = serde_json::json!(t);
                        }
                        items.push(item);
                    }
                    in_item = false;
                }
                b"title" => in_title = false,
                b"pubDate" => in_pub_date = false,
                _ => {}
            },
            Ok(quick_xml::events::Event::Text(e)) => {
                if in_title {
                    current_title = e.unescape().unwrap_or_default().to_string();
                } else if in_pub_date {
                    current_pub_date = e.unescape().unwrap_or_default().to_string();
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    items.truncate(5);
    Ok(items)
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
    pub catalyst_time: Option<i64>,
    pub news_headlines: Vec<NewsHeadline>,
}

/// Fetch enrichment data for a single symbol (requires pre-fetched auth).
pub async fn fetch_enrichment_with_auth(
    client: &Client,
    symbol: &str,
    auth: &YahooAuth,
) -> EnrichmentData {
    let mut data = EnrichmentData::default();

    let (info_result, rss_result, search_result) = tokio::join!(
        fetch_yahoo_info(client, symbol, auth),
        fetch_yahoo_news_rss(client, symbol),
        fetch_yahoo_news(client, symbol, auth)
    );

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

    // Prefer RSS feed for news (more reliable), fall back to search API
    let news = match rss_result {
        Ok(rss) if !rss.is_empty() => rss,
        _ => {
            debug!("RSS news empty for {symbol}, trying search API");
            search_result.unwrap_or_default()
        }
    };

    if let Some((cat_title, cat_time)) = classify_catalyst(&news) {
        data.catalyst = Some(cat_title);
        data.catalyst_time = cat_time;
    }
    data.news_headlines = news
        .iter()
        .filter_map(|item| {
            let title = item.get("title")?.as_str()?.to_string();
            let published = item.get("providerPublishTime").and_then(|t| t.as_i64());
            Some(NewsHeadline { title, published })
        })
        .collect();

    data
}

/// Fetch enrichment data for a single symbol (fetches auth internally).
pub async fn fetch_enrichment(client: &Client, symbol: &str) -> EnrichmentData {
    match fetch_yahoo_auth(client).await {
        Ok(auth) => fetch_enrichment_with_auth(client, symbol, &auth).await,
        Err(e) => {
            warn!("Yahoo auth failed: {e}");
            EnrichmentData::default()
        }
    }
}

/// Enrich a list of scan results with Yahoo Finance data.
/// Fetches auth once and reuses for all symbols.
pub async fn enrich_results(results: &mut [ScanResult]) {
    let client = Client::new();

    let auth = match fetch_yahoo_auth(&client).await {
        Ok(a) => a,
        Err(e) => {
            warn!("Yahoo auth failed, skipping enrichment: {e}");
            return;
        }
    };

    let mut handles = Vec::new();
    for r in results.iter() {
        let client = client.clone();
        let symbol = r.symbol.clone();
        let auth = auth.clone();
        handles.push(tokio::spawn(async move {
            fetch_enrichment_with_auth(&client, &symbol, &auth).await
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
