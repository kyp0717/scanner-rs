/// Keywords that indicate a news catalyst for momentum stocks.
const CATALYST_KEYWORDS: &[&str] = &[
    "fda",
    "approval",
    "drug",
    "trial",
    "earnings",
    "revenue",
    "beat",
    "miss",
    "contract",
    "deal",
    "acquisition",
    "merger",
    "offering",
    "patent",
    "partnership",
    "upgrade",
    "price target",
    "dividend",
    "buyback",
    "split",
    "ceo",
    "appointed",
    "resign",
];

/// Classify news items and return the first headline matching a catalyst keyword,
/// along with its publish timestamp (Unix epoch).
///
/// Each news item should have a "title" field and optionally "providerPublishTime".
pub fn classify_catalyst(news: &[serde_json::Value]) -> Option<(String, Option<i64>)> {
    for item in news {
        let title = item
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("");
        let title_lower = title.to_lowercase();
        for kw in CATALYST_KEYWORDS {
            if title_lower.contains(kw) {
                let publish_time = item
                    .get("providerPublishTime")
                    .and_then(|t| t.as_i64());
                return Some((title.to_string(), publish_time));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_classify_catalyst_fda() {
        let news = vec![json!({"title": "FDA Approves New Drug for ACME Corp"})];
        let result = classify_catalyst(&news);
        assert_eq!(result, Some(("FDA Approves New Drug for ACME Corp".to_string(), None)));
    }

    #[test]
    fn test_classify_catalyst_fda_with_timestamp() {
        let news = vec![json!({"title": "FDA Approves New Drug", "providerPublishTime": 1700000000})];
        let result = classify_catalyst(&news);
        assert_eq!(result, Some(("FDA Approves New Drug".to_string(), Some(1700000000))));
    }

    #[test]
    fn test_classify_catalyst_earnings() {
        let news = vec![
            json!({"title": "Stock market rises today"}),
            json!({"title": "ACME beats earnings expectations"}),
        ];
        let result = classify_catalyst(&news);
        assert_eq!(
            result.map(|(t, _)| t),
            Some("ACME beats earnings expectations".to_string())
        );
    }

    #[test]
    fn test_classify_catalyst_none() {
        let news = vec![json!({"title": "Nothing interesting happened"})];
        let result = classify_catalyst(&news);
        assert!(result.is_none());
    }

    #[test]
    fn test_classify_catalyst_empty() {
        let result = classify_catalyst(&[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_classify_catalyst_case_insensitive() {
        let news = vec![json!({"title": "CEO Resigns from Company"})];
        let result = classify_catalyst(&news);
        assert!(result.is_some());
    }

    #[test]
    fn test_classify_catalyst_multiple_matches_returns_first() {
        let news = vec![
            json!({"title": "FDA approval announced"}),
            json!({"title": "Earnings beat expectations"}),
        ];
        let result = classify_catalyst(&news);
        assert_eq!(result.map(|(t, _)| t), Some("FDA approval announced".to_string()));
    }

    #[test]
    fn test_classify_catalyst_missing_title_field() {
        let news = vec![json!({"headline": "FDA approval"})];
        let result = classify_catalyst(&news);
        assert!(result.is_none());
    }

    #[test]
    fn test_all_keywords_match() {
        for kw in CATALYST_KEYWORDS {
            let news = vec![json!({"title": format!("Something about {kw} happened")})];
            let result = classify_catalyst(&news);
            assert!(result.is_some(), "Keyword '{kw}' should match");
        }
    }
}
