//! Trailing-twelve-month (TTM) rollup over a quarterly periodic table.
//!
//! Strict by default: `ttm_quarterly` returns `None` if any of the four
//! newest quarters is missing or has a null value for `field` — we don't
//! silently extrapolate. Use the `_signed` variant for cash-flow fields
//! (capex, dividends paid) where EODHD reports negative numbers.

use crate::analytics::normalization::{field_f64, sorted_dates_desc};
use serde_json::{Map, Value};

/// Sum the four newest quarterly entries' `field`. Returns `None` unless
/// all four are present and parseable.
pub fn ttm_quarterly(quarterly: &Map<String, Value>, field: &str) -> Option<f64> {
    let dates = sorted_dates_desc(quarterly);
    if dates.len() < 4 {
        return None;
    }
    let mut total = 0.0;
    for d in &dates[..4] {
        let v = field_f64(&quarterly[*d], field)?;
        total += v;
    }
    Some(total)
}

/// Same as `ttm_quarterly` but takes the absolute value of each quarter.
/// Use for fields EODHD reports as negative (capex, dividendsPaid,
/// repurchaseOfStock) when you want the magnitude.
pub fn ttm_quarterly_signed(quarterly: &Map<String, Value>, field: &str) -> Option<f64> {
    let dates = sorted_dates_desc(quarterly);
    if dates.len() < 4 {
        return None;
    }
    let mut total = 0.0;
    for d in &dates[..4] {
        let v = field_f64(&quarterly[*d], field)?;
        total += v.abs();
    }
    Some(total)
}

/// Year-over-year growth rate between the most recent quarter and the
/// quarter four periods earlier. Returns `None` if either is missing,
/// or if the prior value is non-positive (avoids meaningless ratios on
/// loss-making bases).
pub fn yoy_growth(quarterly: &Map<String, Value>, field: &str) -> Option<f64> {
    let dates = sorted_dates_desc(quarterly);
    if dates.len() < 5 {
        return None;
    }
    let current = field_f64(&quarterly[dates[0]], field)?;
    let prior = field_f64(&quarterly[dates[4]], field)?;
    if prior <= 0.0 {
        return None;
    }
    Some((current - prior) / prior)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_value, json};

    fn map(v: Value) -> Map<String, Value> {
        from_value(v).unwrap()
    }

    #[test]
    fn ttm_sums_newest_four_quarters() {
        let q = map(json!({
            "2024-12-31": {"rev": "100"},
            "2024-09-30": {"rev": "90"},
            "2024-06-30": {"rev": "80"},
            "2024-03-31": {"rev": "70"},
            "2023-12-31": {"rev": "60"},
        }));
        assert_eq!(ttm_quarterly(&q, "rev"), Some(340.0));
    }

    #[test]
    fn ttm_returns_none_with_fewer_than_four_quarters() {
        let q = map(json!({
            "2024-12-31": {"rev": "100"},
            "2024-09-30": {"rev": "90"},
        }));
        assert!(ttm_quarterly(&q, "rev").is_none());
    }

    #[test]
    fn ttm_returns_none_when_any_of_the_four_is_null() {
        let q = map(json!({
            "2024-12-31": {"rev": "100"},
            "2024-09-30": {"rev": null},
            "2024-06-30": {"rev": "80"},
            "2024-03-31": {"rev": "70"},
        }));
        assert!(ttm_quarterly(&q, "rev").is_none());
    }

    #[test]
    fn ttm_signed_takes_absolute_value() {
        let q = map(json!({
            "2024-12-31": {"capex": "-2940000000"},
            "2024-09-30": {"capex": "-2910000000"},
            "2024-06-30": {"capex": "-2150000000"},
            "2024-03-31": {"capex": "-2110000000"},
        }));
        assert_eq!(ttm_quarterly_signed(&q, "capex"), Some(10110000000.0));
    }

    #[test]
    fn yoy_growth_correctly_compares_q1_to_q5() {
        let q = map(json!({
            "2024-12-31": {"rev": "120"},
            "2024-09-30": {"rev": "100"},
            "2024-06-30": {"rev": "100"},
            "2024-03-31": {"rev": "100"},
            "2023-12-31": {"rev": "100"},
        }));
        // 2024-12-31 vs 2023-12-31 → 20% growth
        assert!((yoy_growth(&q, "rev").unwrap() - 0.20).abs() < 1e-9);
    }

    #[test]
    fn yoy_growth_returns_none_when_prior_non_positive() {
        let q = map(json!({
            "2024-12-31": {"rev": "100"},
            "2024-09-30": {"rev": "90"},
            "2024-06-30": {"rev": "80"},
            "2024-03-31": {"rev": "70"},
            "2023-12-31": {"rev": "0"},
        }));
        assert!(yoy_growth(&q, "rev").is_none());
    }
}
