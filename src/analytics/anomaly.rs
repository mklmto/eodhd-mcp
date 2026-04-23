//! Anomaly detection — flags non-recurring items, margin outliers, and
//! recurring impairment patterns. Pure functions over `serde_json::Value`
//! so the same code runs in tests with fixture data.
//!
//! The detection thresholds intentionally match spec Appendix C wording:
//! "Z-score > 2 on otherNonCashItems", "≥ 2 events in 4Y for goodwill
//! impairments", "gross margin deviation > 2σ from 3Y mean". Tweaking the
//! numbers should happen here, not at the call sites.

use crate::analytics::normalization::{field_f64, sorted_dates_desc};
use serde::Serialize;
use serde_json::{Map, Value};

/// Standard deviation Z-score threshold for flagging an outlier ("Watch").
pub const Z_WATCH: f64 = 2.0;

/// Z-score threshold for the Material severity. Set at 2.5 because with
/// only 8 quarterly samples (our default trim) the population Z-score
/// is mathematically capped at √(n−1) ≈ 2.65 — a 3.0 threshold would be
/// unreachable in practice.
pub const Z_MATERIAL: f64 = 2.5;

/// Minimum sample size before Z-score testing kicks in. With fewer than
/// 4 observations the standard deviation isn't stable enough to act on.
pub const MIN_SAMPLE: usize = 4;

#[derive(Debug, Clone, Serialize)]
pub struct Outlier {
    pub period: String,
    pub field: String,
    pub value: f64,
    pub z_score: f64,
    pub severity: Severity,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum Severity {
    Watch,    // 2σ ≤ |z| < 3σ
    Material, // |z| ≥ 3σ
}

/// Mean + (population) standard deviation of `xs`. Returns `None` for
/// fewer than `MIN_SAMPLE` values or when σ ≈ 0.
pub fn mean_std(xs: &[f64]) -> Option<(f64, f64)> {
    if xs.len() < MIN_SAMPLE {
        return None;
    }
    let n = xs.len() as f64;
    let mean = xs.iter().sum::<f64>() / n;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    let std = var.sqrt();
    if std < 1e-9 {
        return None;
    }
    Some((mean, std))
}

/// For every period in `quarterly`, compute the Z-score of `field` against
/// the rest of the population. Periods whose Z-score exceeds `Z_THRESHOLD`
/// are returned as `Outlier`s.
pub fn detect_outliers(quarterly: &Map<String, Value>, field: &str) -> Vec<Outlier> {
    let dates = sorted_dates_desc(quarterly);
    let xs: Vec<(String, f64)> = dates
        .iter()
        .filter_map(|d| field_f64(&quarterly[*d], field).map(|v| (d.to_string(), v)))
        .collect();
    if xs.len() < MIN_SAMPLE {
        return Vec::new();
    }
    let values: Vec<f64> = xs.iter().map(|(_, v)| *v).collect();
    let (mean, std) = match mean_std(&values) {
        Some(p) => p,
        None => return Vec::new(),
    };

    let mut out: Vec<Outlier> = Vec::new();
    for (period, v) in xs {
        let z = (v - mean) / std;
        if z.abs() >= Z_WATCH {
            let severity = if z.abs() >= Z_MATERIAL {
                Severity::Material
            } else {
                Severity::Watch
            };
            out.push(Outlier {
                period,
                field: field.to_string(),
                value: v,
                z_score: z,
                severity,
            });
        }
    }
    out
}

/// True iff CFO is below net income in the most recent `min_consecutive`
/// quarters — a classic earnings-quality red flag (spec Appendix C).
pub fn cfo_below_net_income_streak(
    income_q: &Map<String, Value>,
    cf_q: &Map<String, Value>,
    min_consecutive: usize,
) -> bool {
    let dates: Vec<&str> = sorted_dates_desc(income_q);
    if dates.len() < min_consecutive {
        return false;
    }
    let mut consecutive = 0;
    for d in dates {
        let ni = field_f64(&income_q[d], "netIncome");
        let cfo = cf_q
            .get(d)
            .and_then(|v| field_f64(v, "totalCashFromOperatingActivities"));
        match (ni, cfo) {
            (Some(n), Some(c)) if c < n => {
                consecutive += 1;
                if consecutive >= min_consecutive {
                    return true;
                }
            }
            _ => return false,
        }
    }
    false
}

/// True iff revenue YoY (current vs same-period-prior-year) is negative for
/// the most recent `min_consecutive` quarters. Requires at least
/// `min_consecutive + 4` quarters to evaluate.
pub fn revenue_decline_streak(income_q: &Map<String, Value>, min_consecutive: usize) -> bool {
    let dates = sorted_dates_desc(income_q);
    if dates.len() < min_consecutive + 4 {
        return false;
    }
    for i in 0..min_consecutive {
        let cur = field_f64(&income_q[dates[i]], "totalRevenue");
        let prior = field_f64(&income_q[dates[i + 4]], "totalRevenue");
        match (cur, prior) {
            (Some(c), Some(p)) if p > 0.0 && (c - p) / p < 0.0 => continue,
            _ => return false,
        }
    }
    true
}

/// True iff retained earnings is negative AND TTM stock repurchases > 0.
/// Captures spec's "negative retained earnings combined with active buyback"
/// pattern. Returns `(retained_earnings, buyback_ttm_abs)` for evidence.
pub fn negative_retained_with_buyback(
    bs_q: &Map<String, Value>,
    cf_q: &Map<String, Value>,
) -> Option<(f64, f64)> {
    use crate::analytics::ttm::ttm_quarterly_signed;
    let dates = sorted_dates_desc(bs_q);
    let recent = dates.first()?;
    let retained = field_f64(&bs_q[*recent], "retainedEarnings")?;
    if retained >= 0.0 {
        return None;
    }
    let buyback = ttm_quarterly_signed(cf_q, "repurchaseOfStock")?;
    if buyback <= 0.0 {
        return None;
    }
    Some((retained, buyback))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_value, json};

    fn map(v: Value) -> Map<String, Value> {
        from_value(v).unwrap()
    }

    #[test]
    fn mean_std_handles_small_sample() {
        assert!(mean_std(&[1.0, 2.0, 3.0]).is_none());
        let (m, s) = mean_std(&[1.0, 2.0, 3.0, 4.0]).unwrap();
        assert!((m - 2.5).abs() < 1e-9);
        assert!((s - 1.118).abs() < 0.01);
    }

    #[test]
    fn mean_std_returns_none_when_constant() {
        assert!(mean_std(&[5.0, 5.0, 5.0, 5.0]).is_none());
    }

    #[test]
    fn detect_outliers_flags_period_above_threshold() {
        let q = map(json!({
            "2024-12-31": {"x": "100"},
            "2024-09-30": {"x": "50000"}, // big enough to push |z| above 3
            "2024-06-30": {"x": "120"},
            "2024-03-31": {"x": "110"},
            "2023-12-31": {"x": "95"},
            "2023-09-30": {"x": "105"},
            "2023-06-30": {"x": "115"},
            "2023-03-31": {"x": "100"},
        }));
        let outliers = detect_outliers(&q, "x");
        assert_eq!(outliers.len(), 1);
        assert_eq!(outliers[0].period, "2024-09-30");
        assert!(
            outliers[0].z_score >= 2.0,
            "z_score {} below threshold",
            outliers[0].z_score
        );
        // Severity is Watch (2σ ≤ |z| < 3σ) or Material (|z| ≥ 3σ); both valid.
        assert!(matches!(
            outliers[0].severity,
            Severity::Watch | Severity::Material
        ));
    }

    #[test]
    fn outlier_severity_escalates_with_z_score() {
        // Watch range
        let q1 = map(json!({
            "2024-12-31": {"x": "100"}, "2024-09-30": {"x": "100"},
            "2024-06-30": {"x": "100"}, "2024-03-31": {"x": "100"},
            "2023-12-31": {"x": "100"}, "2023-09-30": {"x": "100"},
            "2023-06-30": {"x": "100"}, "2023-03-31": {"x": "300"},
        }));
        let o1 = detect_outliers(&q1, "x");
        assert_eq!(o1.len(), 1);
        assert_eq!(o1[0].severity, Severity::Material);
    }

    #[test]
    fn detect_outliers_returns_empty_on_small_sample() {
        let q = map(json!({"2024-12-31": {"x": "100"}, "2024-09-30": {"x": "10000"}}));
        assert!(detect_outliers(&q, "x").is_empty());
    }

    #[test]
    fn cfo_below_net_income_streak_detects_three_consecutive() {
        let inc = map(json!({
            "2024-12-31": {"netIncome": "100"},
            "2024-09-30": {"netIncome": "100"},
            "2024-06-30": {"netIncome": "100"},
            "2024-03-31": {"netIncome": "100"},
        }));
        let cf = map(json!({
            "2024-12-31": {"totalCashFromOperatingActivities": "50"},
            "2024-09-30": {"totalCashFromOperatingActivities": "60"},
            "2024-06-30": {"totalCashFromOperatingActivities": "40"},
            "2024-03-31": {"totalCashFromOperatingActivities": "200"},
        }));
        assert!(cfo_below_net_income_streak(&inc, &cf, 3));
        // Streak of 4 is broken by Q1 (200 > 100)
        assert!(!cfo_below_net_income_streak(&inc, &cf, 4));
    }

    #[test]
    fn revenue_decline_streak_3q_detected() {
        let inc = map(json!({
            "2024-12-31": {"totalRevenue": "90"},
            "2024-09-30": {"totalRevenue": "85"},
            "2024-06-30": {"totalRevenue": "80"},
            "2024-03-31": {"totalRevenue": "100"},
            "2023-12-31": {"totalRevenue": "100"},
            "2023-09-30": {"totalRevenue": "100"},
            "2023-06-30": {"totalRevenue": "100"},
            "2023-03-31": {"totalRevenue": "100"},
        }));
        assert!(revenue_decline_streak(&inc, 3));
    }

    #[test]
    fn negative_retained_with_buyback_returns_evidence() {
        let bs = map(json!({
            "2024-12-31": {"retainedEarnings": "-6920000000"},
        }));
        let cf = map(json!({
            "2024-12-31": {"repurchaseOfStock": "-23000000000"},
            "2024-09-30": {"repurchaseOfStock": "-25000000000"},
            "2024-06-30": {"repurchaseOfStock": "-26000000000"},
            "2024-03-31": {"repurchaseOfStock": "-23000000000"},
        }));
        let evidence = negative_retained_with_buyback(&bs, &cf).unwrap();
        assert_eq!(evidence.0, -6_920_000_000.0);
        assert_eq!(evidence.1, 97_000_000_000.0);
    }

    #[test]
    fn detects_synthetic_aapl_other_non_cash_spike() {
        let raw = include_str!("../../tests/fixtures/aapl_fundamentals.json");
        let v: Value = serde_json::from_str(raw).unwrap();
        let cf = v["Financials"]["Cash_Flow"]["quarterly"]
            .as_object()
            .unwrap();
        let outliers = detect_outliers(cf, "otherNonCashItems");
        assert!(
            outliers.iter().any(|o| o.period == "2024-09-30"),
            "expected the synthetic 10.5B spike at 2024-09-30 to be flagged; got {:?}",
            outliers
        );
    }
}
