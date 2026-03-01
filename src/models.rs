use serde::{Deserialize, Serialize};

/// A news headline with optional publish timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsHeadline {
    pub title: String,
    pub published: Option<i64>,
}

/// Result from a TWS scanner + enrichment.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanResult {
    pub rank: u32,
    pub symbol: String,
    pub con_id: i64,
    pub exchange: String,
    pub currency: String,
    pub last: Option<f64>,
    pub change: Option<f64>,
    pub change_pct: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub volume: Option<i64>,
    pub close: Option<f64>,
    // Enrichment fields (from Yahoo Finance)
    pub name: Option<String>,
    pub sector: Option<String>,
    pub industry: Option<String>,
    pub float_shares: Option<f64>,
    pub short_pct: Option<f64>,
    pub avg_volume: Option<i64>,
    pub catalyst: Option<String>,
    pub rvol: Option<f64>,
}

/// Row in the alert table (accumulated during polling).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRow {
    pub symbol: String,
    pub alert_time: String,
    pub last: Option<f64>,
    pub change_pct: Option<f64>,
    pub volume: Option<i64>,
    pub rvol: Option<f64>,
    pub float_shares: Option<f64>,
    pub short_pct: Option<f64>,
    pub name: Option<String>,
    pub sector: Option<String>,
    pub industry: Option<String>,
    pub catalyst: Option<String>,
    pub catalyst_time: Option<i64>,
    pub scanner_hits: u32,
    pub news_headlines: Vec<NewsHeadline>,
    pub enriched: bool,
    pub avg_volume: Option<i64>,
}

/// A sighting row from Supabase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sighting {
    pub id: Option<i64>,
    pub symbol: String,
    pub first_seen: String,
    pub last_seen: String,
    pub scanners: String,
    pub hit_count: Option<i32>,
    pub last_price: Option<f64>,
    pub change_pct: Option<f64>,
    pub rvol: Option<f64>,
    pub float_shares: Option<f64>,
    pub catalyst: Option<String>,
    pub name: Option<String>,
    pub sector: Option<String>,
    // Enrichment cache fields
    pub enriched_at: Option<String>,
    pub industry: Option<String>,
    pub short_pct: Option<f64>,
    pub avg_volume: Option<i64>,
    pub news_headlines: Option<String>, // JSON array string
}

/// Application settings.
#[derive(Debug, Clone)]
pub struct Settings {
    pub port: Option<u16>,
    pub host: String,
    pub rows: u32,
    pub min_price: Option<f64>,
    pub max_price: Option<f64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            port: None,
            host: "127.0.0.1".to_string(),
            rows: 25,
            min_price: Some(1.0),
            max_price: None,
        }
    }
}

/// Scanner alias mapping.
pub fn resolve_scanner(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "gain" => "TOP_PERC_GAIN".to_string(),
        "hot" => "HOT_BY_VOLUME".to_string(),
        "active" => "MOST_ACTIVE".to_string(),
        "lose" => "TOP_PERC_LOSE".to_string(),
        "gap" => "HIGH_OPEN_GAP".to_string(),
        "gapdown" => "LOW_OPEN_GAP".to_string(),
        other => other.to_uppercase(),
    }
}

/// All scanner aliases.
pub const ALIASES: &[(&str, &str)] = &[
    ("gain", "TOP_PERC_GAIN"),
    ("hot", "HOT_BY_VOLUME"),
    ("active", "MOST_ACTIVE"),
    ("lose", "TOP_PERC_LOSE"),
    ("gap", "HIGH_OPEN_GAP"),
    ("gapdown", "LOW_OPEN_GAP"),
];

/// Alert scanners with their client IDs.
pub const ALERT_SCANNERS: &[(&str, i32)] = &[
    ("HOT_BY_VOLUME", 10),
    ("TOP_PERC_GAIN", 11),
    ("MOST_ACTIVE", 12),
    ("HIGH_OPEN_GAP", 13),
    ("TOP_TRADE_COUNT", 14),
    ("HOT_BY_PRICE", 15),
    ("TOP_VOLUME_RATE", 16),
    ("HIGH_VS_52W_HL", 17),
];

pub const DEFAULT_PORTS: &[u16] = &[7500, 7497];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_scanner_alias() {
        assert_eq!(resolve_scanner("gain"), "TOP_PERC_GAIN");
        assert_eq!(resolve_scanner("hot"), "HOT_BY_VOLUME");
        assert_eq!(resolve_scanner("active"), "MOST_ACTIVE");
        assert_eq!(resolve_scanner("lose"), "TOP_PERC_LOSE");
        assert_eq!(resolve_scanner("gap"), "HIGH_OPEN_GAP");
        assert_eq!(resolve_scanner("gapdown"), "LOW_OPEN_GAP");
    }

    #[test]
    fn test_resolve_scanner_passthrough() {
        assert_eq!(resolve_scanner("TOP_PERC_GAIN"), "TOP_PERC_GAIN");
        assert_eq!(resolve_scanner("some_custom"), "SOME_CUSTOM");
    }

    #[test]
    fn test_settings_default() {
        let s = Settings::default();
        assert_eq!(s.host, "127.0.0.1");
        assert_eq!(s.rows, 25);
        assert!(s.port.is_none());
        assert_eq!(s.min_price, Some(1.0));
        assert!(s.max_price.is_none());
    }

    #[test]
    fn test_sighting_new_fields() {
        let s = Sighting {
            id: None,
            symbol: "TEST".to_string(),
            first_seen: "2024-01-01T00:00:00+00:00".to_string(),
            last_seen: "2024-01-01T00:00:00+00:00".to_string(),
            scanners: "".to_string(),
            hit_count: None,
            last_price: None,
            change_pct: None,
            rvol: None,
            float_shares: None,
            catalyst: None,
            name: None,
            sector: None,
            enriched_at: Some("2024-01-01T00:00:00+00:00".to_string()),
            industry: Some("Biotech".to_string()),
            short_pct: Some(0.15),
            avg_volume: Some(1_000_000),
            news_headlines: Some(r#"[{"title":"Headline 1","published":null}]"#.to_string()),
        };
        assert_eq!(s.industry, Some("Biotech".to_string()));
        assert_eq!(s.avg_volume, Some(1_000_000));
    }

    #[test]
    fn test_scan_result_default() {
        let r = ScanResult::default();
        assert_eq!(r.rank, 0);
        assert!(r.symbol.is_empty());
        assert!(r.last.is_none());
    }
}
