use crate::models::ScanResult;

/// Filter results to only stocks passing all 5 momentum pillars.
///
/// 1. Price $1-$20
/// 2. Change >= 10%
/// 3. Relative Volume >= 5x
/// 4. Float < 10M (skip if unknown)
/// 5. Has news catalyst
pub fn filter_momentum(results: &[ScanResult]) -> Vec<ScanResult> {
    results
        .iter()
        .filter(|r| {
            let price = match r.last {
                Some(p) => p,
                None => return false,
            };
            let chg = match r.change_pct {
                Some(c) => c,
                None => return false,
            };
            // Price: $1-$20
            if !(1.0..=20.0).contains(&price) {
                return false;
            }
            // Change: >= 10%
            if chg < 10.0 {
                return false;
            }
            // RVol: >= 5x
            match r.rvol {
                Some(rv) if rv >= 5.0 => {}
                _ => return false,
            }
            // Float: < 10M (skip if None)
            if let Some(flt) = r.float_shares {
                if flt >= 10_000_000.0 {
                    return false;
                }
            }
            // Catalyst: must be present
            if r.catalyst.is_none() {
                return false;
            }
            true
        })
        .cloned()
        .collect()
}

/// Format a price value for display.
pub fn fmt_price(price: Option<f64>) -> String {
    match price {
        Some(p) => format!("{p:.2}"),
        None => "-".to_string(),
    }
}

/// Format a change percentage for display.
pub fn fmt_change_pct(pct: Option<f64>) -> String {
    match pct {
        Some(p) => format!("{p:+.1}%"),
        None => "-".to_string(),
    }
}

/// Format volume for display (with commas).
pub fn fmt_volume(vol: Option<i64>) -> String {
    match vol {
        Some(v) => {
            // Simple comma formatting
            let s = v.to_string();
            let bytes = s.as_bytes();
            let mut result = String::new();
            for (i, &b) in bytes.iter().enumerate() {
                if i > 0 && (bytes.len() - i) % 3 == 0 {
                    result.push(',');
                }
                result.push(b as char);
            }
            result
        }
        None => "-".to_string(),
    }
}

/// Format relative volume for display.
pub fn fmt_rvol(rvol: Option<f64>) -> String {
    match rvol {
        Some(r) => format!("{r:.1}x"),
        None => "-".to_string(),
    }
}

/// Format float shares for display (in millions).
pub fn fmt_float(float_shares: Option<f64>) -> String {
    match float_shares {
        Some(f) => format!("{:.1}M", f / 1e6),
        None => "-".to_string(),
    }
}

/// Format short percentage for display.
pub fn fmt_short_pct(pct: Option<f64>) -> String {
    match pct {
        Some(p) => format!("{:.1}%", p * 100.0),
        None => "-".to_string(),
    }
}

/// Truncate a string to max_len, adding ".." if truncated.
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}..", &s[..max_len.saturating_sub(2)])
    } else {
        s.to_string()
    }
}

/// Print scan results as a formatted table to stdout.
pub fn print_results(results: &[ScanResult]) {
    if results.is_empty() {
        println!("No results.");
        return;
    }

    let has_live = results.iter().any(|r| r.last.is_some());

    if has_live {
        println!(
            "{:>3}  {:<6}  {:>8}  {:>8}  {:>12}  {:>6}  {:>8}  {:>7}  {:<20}  {:<14}  {}",
            "#", "Symbol", "Last", "Chg%", "Volume", "RVol", "Float", "Short%", "Name", "Sector", "Catalyst"
        );
        println!("{}", "-".repeat(120));

        for r in results {
            let name = r.name.as_deref().unwrap_or("-");
            let sector = r.sector.as_deref().unwrap_or("-");
            let catalyst = r.catalyst.as_deref().unwrap_or("");
            println!(
                "{:>3}  {:<6}  {:>8}  {:>8}  {:>12}  {:>6}  {:>8}  {:>7}  {:<20}  {:<14}  {}",
                r.rank,
                r.symbol,
                fmt_price(r.last),
                fmt_change_pct(r.change_pct),
                fmt_volume(r.volume),
                fmt_rvol(r.rvol),
                fmt_float(r.float_shares),
                fmt_short_pct(r.short_pct),
                truncate(name, 20),
                truncate(sector, 14),
                truncate(catalyst, 30),
            );
        }
    } else {
        println!("(Market closed -- showing previous close prices)");
        println!("{:>3}  {:<6}  {:>8}", "#", "Symbol", "Close");
        println!("{}", "-".repeat(24));
        for r in results {
            println!(
                "{:>3}  {:<6}  {:>8}",
                r.rank,
                r.symbol,
                fmt_price(r.close),
            );
        }
    }

    println!("\nTotal: {} stocks", results.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(
        last: Option<f64>,
        change_pct: Option<f64>,
        rvol: Option<f64>,
        float_shares: Option<f64>,
        catalyst: Option<&str>,
    ) -> ScanResult {
        ScanResult {
            rank: 1,
            symbol: "TEST".to_string(),
            last,
            change_pct,
            rvol,
            float_shares,
            catalyst: catalyst.map(|s| s.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn test_filter_momentum_pass() {
        let results = vec![make_result(
            Some(5.0),
            Some(15.0),
            Some(6.0),
            Some(5_000_000.0),
            Some("FDA approval"),
        )];
        let filtered = filter_momentum(&results);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_momentum_fail_price_too_high() {
        let results = vec![make_result(
            Some(25.0),
            Some(15.0),
            Some(6.0),
            Some(5_000_000.0),
            Some("FDA approval"),
        )];
        let filtered = filter_momentum(&results);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_momentum_fail_price_too_low() {
        let results = vec![make_result(
            Some(0.5),
            Some(15.0),
            Some(6.0),
            Some(5_000_000.0),
            Some("FDA approval"),
        )];
        let filtered = filter_momentum(&results);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_momentum_fail_change_low() {
        let results = vec![make_result(
            Some(5.0),
            Some(5.0),
            Some(6.0),
            Some(5_000_000.0),
            Some("FDA approval"),
        )];
        let filtered = filter_momentum(&results);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_momentum_fail_rvol_low() {
        let results = vec![make_result(
            Some(5.0),
            Some(15.0),
            Some(3.0),
            Some(5_000_000.0),
            Some("FDA approval"),
        )];
        let filtered = filter_momentum(&results);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_momentum_fail_float_high() {
        let results = vec![make_result(
            Some(5.0),
            Some(15.0),
            Some(6.0),
            Some(15_000_000.0),
            Some("FDA approval"),
        )];
        let filtered = filter_momentum(&results);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_momentum_fail_no_catalyst() {
        let results = vec![make_result(
            Some(5.0),
            Some(15.0),
            Some(6.0),
            Some(5_000_000.0),
            None,
        )];
        let filtered = filter_momentum(&results);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_momentum_unknown_float_passes() {
        // Float None should still pass (skip check)
        let results = vec![make_result(
            Some(5.0),
            Some(15.0),
            Some(6.0),
            None,
            Some("FDA approval"),
        )];
        let filtered = filter_momentum(&results);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_momentum_no_price() {
        let results = vec![make_result(
            None,
            Some(15.0),
            Some(6.0),
            Some(5_000_000.0),
            Some("FDA approval"),
        )];
        let filtered = filter_momentum(&results);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_momentum_no_change() {
        let results = vec![make_result(
            Some(5.0),
            None,
            Some(6.0),
            Some(5_000_000.0),
            Some("FDA approval"),
        )];
        let filtered = filter_momentum(&results);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_fmt_price() {
        assert_eq!(fmt_price(Some(12.345)), "12.35");
        assert_eq!(fmt_price(None), "-");
    }

    #[test]
    fn test_fmt_change_pct() {
        assert_eq!(fmt_change_pct(Some(15.0)), "+15.0%");
        assert_eq!(fmt_change_pct(Some(-5.3)), "-5.3%");
        assert_eq!(fmt_change_pct(None), "-");
    }

    #[test]
    fn test_fmt_volume() {
        assert_eq!(fmt_volume(Some(1234567)), "1,234,567");
        assert_eq!(fmt_volume(Some(100)), "100");
        assert_eq!(fmt_volume(None), "-");
    }

    #[test]
    fn test_fmt_rvol() {
        assert_eq!(fmt_rvol(Some(5.3)), "5.3x");
        assert_eq!(fmt_rvol(None), "-");
    }

    #[test]
    fn test_fmt_float() {
        assert_eq!(fmt_float(Some(5_000_000.0)), "5.0M");
        assert_eq!(fmt_float(None), "-");
    }

    #[test]
    fn test_fmt_short_pct() {
        assert_eq!(fmt_short_pct(Some(0.15)), "15.0%");
        assert_eq!(fmt_short_pct(None), "-");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("a very long string here", 10), "a very l..");
    }

    #[test]
    fn test_filter_momentum_boundary_price() {
        // Price exactly 1.0 should pass
        let results = vec![make_result(
            Some(1.0),
            Some(15.0),
            Some(6.0),
            Some(5_000_000.0),
            Some("FDA approval"),
        )];
        assert_eq!(filter_momentum(&results).len(), 1);

        // Price exactly 20.0 should pass
        let results = vec![make_result(
            Some(20.0),
            Some(15.0),
            Some(6.0),
            Some(5_000_000.0),
            Some("FDA approval"),
        )];
        assert_eq!(filter_momentum(&results).len(), 1);
    }

    #[test]
    fn test_filter_momentum_boundary_change() {
        // Change exactly 10.0 should pass
        let results = vec![make_result(
            Some(5.0),
            Some(10.0),
            Some(6.0),
            Some(5_000_000.0),
            Some("FDA approval"),
        )];
        assert_eq!(filter_momentum(&results).len(), 1);

        // Change 9.9 should fail
        let results = vec![make_result(
            Some(5.0),
            Some(9.9),
            Some(6.0),
            Some(5_000_000.0),
            Some("FDA approval"),
        )];
        assert!(filter_momentum(&results).is_empty());
    }

    #[test]
    fn test_filter_momentum_boundary_rvol() {
        // RVol exactly 5.0 should pass
        let results = vec![make_result(
            Some(5.0),
            Some(15.0),
            Some(5.0),
            Some(5_000_000.0),
            Some("FDA approval"),
        )];
        assert_eq!(filter_momentum(&results).len(), 1);

        // RVol 4.9 should fail
        let results = vec![make_result(
            Some(5.0),
            Some(15.0),
            Some(4.9),
            Some(5_000_000.0),
            Some("FDA approval"),
        )];
        assert!(filter_momentum(&results).is_empty());
    }
}
