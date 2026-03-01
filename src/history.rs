use anyhow::{Context, Result};
use chrono::{Local, Utc};
use reqwest::Client;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::config::SupabaseConfig;
use crate::enrichment::EnrichmentData;
use crate::models::Sighting;

const TABLE: &str = "sightings";

/// Supabase REST API client for the sightings table.
#[derive(Clone)]
pub struct SupabaseClient {
    client: Client,
    config: SupabaseConfig,
}

impl SupabaseClient {
    pub fn new(config: SupabaseConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    /// Reconnect (create fresh HTTP client).
    pub fn reconnect(&mut self) {
        info!("Reconnecting to Supabase...");
        self.client = Client::new();
    }

    fn base_url(&self) -> String {
        format!("{}/rest/v1/{TABLE}", self.config.url)
    }

    fn auth_headers(&self) -> Vec<(&str, String)> {
        vec![
            ("apikey", self.config.anon_key.clone()),
            ("Authorization", format!("Bearer {}", self.config.anon_key)),
        ]
    }

    /// SELECT rows with optional filters.
    async fn select(&self, query: &str) -> Result<Vec<Value>> {
        let url = format!("{}?{query}", self.base_url());
        let mut req = self.client.get(&url);
        for (k, v) in self.auth_headers() {
            req = req.header(k, v);
        }
        let resp = req.send().await.context("Supabase SELECT failed")?;
        let data: Vec<Value> = resp.json().await.context("Supabase response parse failed")?;
        Ok(data)
    }

    /// INSERT rows.
    async fn insert(&self, rows: &[Value]) -> Result<()> {
        let mut req = self.client.post(&self.base_url());
        for (k, v) in self.auth_headers() {
            req = req.header(k, v);
        }
        req = req.header("Content-Type", "application/json");
        req = req.header("Prefer", "return=minimal");
        req.json(rows)
            .send()
            .await
            .context("Supabase INSERT failed")?;
        Ok(())
    }

    /// UPDATE rows matching a filter.
    async fn update(&self, filter: &str, data: &Value) -> Result<()> {
        let url = format!("{}?{filter}", self.base_url());
        let mut req = self.client.patch(&url);
        for (k, v) in self.auth_headers() {
            req = req.header(k, v);
        }
        req = req.header("Content-Type", "application/json");
        req = req.header("Prefer", "return=minimal");
        req.json(data)
            .send()
            .await
            .context("Supabase UPDATE failed")?;
        Ok(())
    }

    /// DELETE rows matching a filter.
    async fn delete(&self, filter: &str) -> Result<()> {
        let url = format!("{}?{filter}", self.base_url());
        let mut req = self.client.delete(&url);
        for (k, v) in self.auth_headers() {
            req = req.header(k, v);
        }
        req.send().await.context("Supabase DELETE failed")?;
        Ok(())
    }

    /// Record a batch of stock sightings (insert new, update existing).
    /// stocks: map of symbol -> (data, scanners list)
    pub async fn record_stocks_batch(
        &mut self,
        stocks: &std::collections::HashMap<String, (Value, Vec<String>)>,
    ) -> Result<()> {
        if stocks.is_empty() {
            return Ok(());
        }

        let symbols: Vec<&str> = stocks.keys().map(|s| s.as_str()).collect();
        let now = Local::now().format("%Y-%m-%dT%H:%M:%S%:z").to_string();

        for attempt in 0..3 {
            match self.try_record_batch(&symbols, stocks, &now).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    let msg = format!("{e}");
                    if attempt < 2
                        && (msg.contains("connection")
                            || msg.contains("Connection")
                            || msg.contains("reset"))
                    {
                        warn!("Supabase connection dropped, reconnecting (attempt {})...", attempt + 1);
                        self.reconnect();
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        continue;
                    }
                    warn!("Supabase record_stocks_batch failed: {e}");
                    return Ok(()); // Don't crash
                }
            }
        }
        Ok(())
    }

    async fn try_record_batch(
        &self,
        symbols: &[&str],
        stocks: &std::collections::HashMap<String, (Value, Vec<String>)>,
        now: &str,
    ) -> Result<()> {
        // Bulk SELECT existing symbols
        let symbols_param = symbols
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(",");
        let query = format!("select=id,symbol,scanners,hit_count&symbol=in.({symbols_param})");
        let existing = self.select(&query).await?;

        let existing_map: std::collections::HashMap<String, Value> = existing
            .into_iter()
            .filter_map(|row| {
                let sym = row.get("symbol")?.as_str()?.to_string();
                Some((sym, row))
            })
            .collect();

        // Separate inserts and updates
        let mut inserts = Vec::new();
        for (sym, (data, scanner_list)) in stocks {
            let scanners_str = {
                let mut set: std::collections::BTreeSet<&str> =
                    scanner_list.iter().map(|s| s.as_str()).collect();
                // Merge with existing scanners if present
                if let Some(existing_row) = existing_map.get(sym) {
                    if let Some(existing_scanners) = existing_row.get("scanners").and_then(|s| s.as_str()) {
                        for s in existing_scanners.split(',') {
                            set.insert(s);
                        }
                    }
                }
                set.into_iter().collect::<Vec<_>>().join(",")
            };

            if existing_map.contains_key(sym) {
                let existing_row = &existing_map[sym];
                let old_hits = existing_row
                    .get("hit_count")
                    .and_then(|h| h.as_i64())
                    .unwrap_or(0);

                let mut update = json!({
                    "last_seen": now,
                    "scanners": scanners_str,
                    "hit_count": old_hits + scanner_list.len() as i64,
                });

                // Only update fields with non-null values
                for (db_col, data_key) in &[
                    ("last_price", "last"),
                    ("change_pct", "change_pct"),
                    ("rvol", "rvol"),
                    ("float_shares", "float_shares"),
                    ("catalyst", "catalyst"),
                    ("name", "name"),
                    ("sector", "sector"),
                    ("industry", "industry"),
                    ("short_pct", "short_pct"),
                    ("avg_volume", "avg_volume"),
                    ("news_headlines", "news_headlines"),
                    ("enriched_at", "enriched_at"),
                ] {
                    if let Some(val) = data.get(data_key) {
                        if !val.is_null() {
                            update[db_col] = val.clone();
                        }
                    }
                }

                let filter = format!("symbol=eq.{sym}");
                self.update(&filter, &update).await?;
            } else {
                let mut insert = json!({
                    "symbol": sym,
                    "first_seen": now,
                    "last_seen": now,
                    "scanners": scanners_str,
                    "hit_count": scanner_list.len(),
                    "last_price": data.get("last").cloned().unwrap_or(Value::Null),
                    "change_pct": data.get("change_pct").cloned().unwrap_or(Value::Null),
                    "rvol": data.get("rvol").cloned().unwrap_or(Value::Null),
                    "float_shares": data.get("float_shares").cloned().unwrap_or(Value::Null),
                    "catalyst": data.get("catalyst").cloned().unwrap_or(Value::Null),
                    "name": data.get("name").cloned().unwrap_or(Value::Null),
                    "sector": data.get("sector").cloned().unwrap_or(Value::Null),
                });
                for key in &["industry", "short_pct", "avg_volume", "news_headlines", "enriched_at"] {
                    if let Some(val) = data.get(key) {
                        if !val.is_null() {
                            insert[key] = val.clone();
                        }
                    }
                }
                inserts.push(insert);
            }
        }

        if !inserts.is_empty() {
            self.insert(&inserts).await?;
        }

        Ok(())
    }

    /// Check enrichment cache for a symbol. Returns Some(EnrichmentData) if
    /// the symbol has been enriched within `max_age`.
    pub async fn get_enrichment_cache(
        &self,
        symbol: &str,
        max_age: std::time::Duration,
    ) -> Option<EnrichmentData> {
        let query = format!(
            "select=name,sector,industry,float_shares,short_pct,avg_volume,catalyst,news_headlines,enriched_at&symbol=eq.{symbol}&limit=1"
        );
        let rows = self.select(&query).await.ok()?;
        let row = rows.into_iter().next()?;

        // Check enriched_at freshness
        let enriched_at_str = row.get("enriched_at")?.as_str()?;
        let enriched_at = chrono::DateTime::parse_from_rfc3339(enriched_at_str).ok()?;
        let age = Utc::now().signed_duration_since(enriched_at.with_timezone(&Utc));
        if age > chrono::Duration::from_std(max_age).ok()? {
            return None;
        }

        // Reconstruct EnrichmentData from cached fields
        let news_headlines: Vec<String> = row
            .get("news_headlines")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        Some(EnrichmentData {
            name: row.get("name").and_then(|v| v.as_str()).map(String::from),
            sector: row.get("sector").and_then(|v| v.as_str()).map(String::from),
            industry: row.get("industry").and_then(|v| v.as_str()).map(String::from),
            float_shares: row.get("float_shares").and_then(|v| v.as_f64()),
            short_pct: row.get("short_pct").and_then(|v| v.as_f64()),
            avg_volume: row.get("avg_volume").and_then(|v| v.as_i64()),
            catalyst: row.get("catalyst").and_then(|v| v.as_str()).map(String::from),
            news_headlines,
        })
    }

    /// Get history (all sightings, ordered by first_seen DESC).
    pub async fn get_history(&self, limit: u32) -> Result<Vec<Sighting>> {
        let query = format!("select=*&order=first_seen.desc&limit={limit}");
        let rows = self.select(&query).await?;
        let sightings = rows
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        Ok(sightings)
    }

    /// Get today's sightings (first_seen >= today midnight).
    pub async fn get_today(&self) -> Result<Vec<Sighting>> {
        let today = Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let midnight = Local::now()
            .timezone()
            .from_local_datetime(&today)
            .single()
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string();

        let query = format!("select=*&first_seen=gte.{midnight}&order=first_seen.desc");
        let rows = self.select(&query).await?;
        let sightings = rows
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        Ok(sightings)
    }

    /// Clear all history. Returns count of deleted rows.
    pub async fn clear_history(&self) -> Result<u32> {
        // Count first
        let count_query = "select=id&limit=10000";
        let rows = self.select(count_query).await?;
        let count = rows.len() as u32;

        // Delete all
        self.delete("symbol=neq.").await?;
        Ok(count)
    }

    /// Get symbols that are not already in the database.
    pub async fn get_new_symbols(&self, symbols: &[String]) -> Result<std::collections::HashSet<String>> {
        if symbols.is_empty() {
            return Ok(std::collections::HashSet::new());
        }
        let symbols_param = symbols
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(",");
        let query = format!("select=symbol&symbol=in.({symbols_param})");
        let rows = self.select(&query).await?;
        let existing: std::collections::HashSet<String> = rows
            .iter()
            .filter_map(|r| r.get("symbol")?.as_str().map(|s| s.to_string()))
            .collect();
        let all: std::collections::HashSet<String> = symbols.iter().cloned().collect();
        Ok(all.difference(&existing).cloned().collect())
    }
}

/// Print sightings as a formatted history table.
pub fn print_history(sightings: &[Sighting], label: &str) {
    if sightings.is_empty() {
        println!("{label}: no stocks in history");
        return;
    }

    println!("{label} -- {} stocks", sightings.len());
    println!(
        "{:<10}  {:<6}  {:>8}  {:>8}  {:>6}  {:<30}  {:>4}  {}",
        "Time", "Symbol", "Last", "Chg%", "RVol", "Scanners", "Hits", "Catalyst"
    );
    println!("{}", "-".repeat(100));

    for s in sightings {
        let time_str = local_time_str(&s.first_seen);
        let price = match s.last_price {
            Some(p) => format!("{p:.2}"),
            None => "-".to_string(),
        };
        let chg = match s.change_pct {
            Some(c) => format!("{c:+.1}%"),
            None => "-".to_string(),
        };
        let rvol = match s.rvol {
            Some(r) => format!("{r:.1}x"),
            None => "-".to_string(),
        };
        let hits = s.hit_count.unwrap_or(0);
        let catalyst = s.catalyst.as_deref().unwrap_or("");
        let catalyst = if catalyst.len() > 30 {
            format!("{}..", &catalyst[..28])
        } else {
            catalyst.to_string()
        };

        println!(
            "{:<10}  {:<6}  {:>8}  {:>8}  {:>6}  {:<30}  {:>4}  {}",
            time_str, s.symbol, price, chg, rvol, s.scanners, hits, catalyst
        );
    }
}

/// Convert an ISO timestamp to local HH:MM:SS.
pub fn local_time_str(iso_ts: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso_ts)
        .or_else(|_| chrono::DateTime::parse_from_str(iso_ts, "%Y-%m-%dT%H:%M:%S%:z"))
        .map(|dt| dt.with_timezone(&Local).format("%H:%M:%S").to_string())
        .unwrap_or_else(|_| {
            if iso_ts.len() >= 8 {
                iso_ts[..8].to_string()
            } else {
                "-".to_string()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_time_str_valid() {
        // Should not panic on valid ISO timestamp
        let result = local_time_str("2024-01-15T14:30:00+00:00");
        assert!(!result.is_empty());
        assert_ne!(result, "-");
    }

    #[test]
    fn test_local_time_str_invalid() {
        assert_eq!(local_time_str("not-a-date"), "not-a-da");
    }

    #[test]
    fn test_local_time_str_short() {
        assert_eq!(local_time_str("abc"), "-");
    }

    #[test]
    fn test_print_history_empty() {
        // Should not panic
        print_history(&[], "Test");
    }

    #[test]
    fn test_print_history_with_data() {
        let sightings = vec![Sighting {
            id: Some(1),
            symbol: "AAPL".to_string(),
            first_seen: "2024-01-15T14:30:00+00:00".to_string(),
            last_seen: "2024-01-15T14:35:00+00:00".to_string(),
            scanners: "HOT_BY_VOLUME,TOP_PERC_GAIN".to_string(),
            hit_count: Some(3),
            last_price: Some(15.50),
            change_pct: Some(12.5),
            rvol: Some(6.3),
            float_shares: Some(5_000_000.0),
            catalyst: Some("FDA approval for new drug".to_string()),
            name: Some("Apple Inc".to_string()),
            sector: Some("Technology".to_string()),
            enriched_at: None,
            industry: None,
            short_pct: None,
            avg_volume: None,
            news_headlines: None,
        }];
        // Should not panic
        print_history(&sightings, "Today");
    }
}

use chrono::TimeZone;
