//! Normalization helpers — date detection, periodic slicing, null tolerance,
//! string-or-number coercion. Pure functions over `serde_json::Value`.

use serde_json::{Map, Value};

/// True if `s` matches the ISO short-date shape `YYYY-MM-DD`. Cheap structural
/// check (no calendar validation) — sufficient for telling EODHD's date-keyed
/// periodic objects apart from regular fields.
pub fn is_iso_date(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 10 {
        return false;
    }
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    bytes
        .iter()
        .enumerate()
        .all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit())
}

/// True if every key in `map` is an ISO date string. Used to recognise
/// the periodic tables that live under `Financials::*::{quarterly,yearly}`.
pub fn is_periodic_map(map: &Map<String, Value>) -> bool {
    !map.is_empty() && map.keys().all(|k| is_iso_date(k))
}

/// In-place recursive slicer. Walks `value`, and for every object whose keys
/// are all ISO dates, applies (in order):
///   1. retain entries with key in `[from, to]` (inclusive, lexical compare —
///      safe because ISO dates sort correctly as strings),
///   2. retain only the `last_n` newest entries (entries are kept regardless
///      of insertion order; `last_n` selects by descending date).
///
/// The non-periodic structure of the JSON tree is preserved verbatim. This
/// lets the caller pass either the whole `/fundamentals/SYMBOL` blob or a
/// pre-filtered sub-tree (e.g. `Financials::Cash_Flow::quarterly`) and get
/// the same trimming behaviour.
pub fn slice_periodic(
    value: &mut Value,
    last_n: Option<usize>,
    from: Option<&str>,
    to: Option<&str>,
) {
    if last_n.is_none() && from.is_none() && to.is_none() {
        return;
    }

    match value {
        Value::Object(map) => {
            if is_periodic_map(map) {
                trim_periodic_map(map, last_n, from, to);
            } else {
                for v in map.values_mut() {
                    slice_periodic(v, last_n, from, to);
                }
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                slice_periodic(v, last_n, from, to);
            }
        }
        _ => {}
    }
}

fn trim_periodic_map(
    map: &mut Map<String, Value>,
    last_n: Option<usize>,
    from: Option<&str>,
    to: Option<&str>,
) {
    // Step 1 — date-range filter.
    if from.is_some() || to.is_some() {
        map.retain(|k, _| {
            from.map_or(true, |f| k.as_str() >= f) && to.map_or(true, |t| k.as_str() <= t)
        });
    }

    // Step 2 — keep newest N. We collect, sort desc by date, decide which
    // keys survive, then re-shape the map preserving the desired order.
    if let Some(n) = last_n {
        let mut keys: Vec<String> = map.keys().cloned().collect();
        keys.sort_by(|a, b| b.cmp(a)); // descending
        let kept: std::collections::HashSet<String> = keys.into_iter().take(n).collect();
        map.retain(|k, _| kept.contains(k));
    }
}

/// Coerce a JSON value to `f64`. EODHD often returns numeric fields as
/// strings ("124300000000"); we accept both. Returns `None` on null,
/// missing, or unparseable.
pub fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

/// Look up `field` from a periodic-table entry and coerce to `f64`.
pub fn field_f64(entry: &Value, field: &str) -> Option<f64> {
    entry.get(field).and_then(as_f64)
}

/// Sort the date keys of a periodic map descending. Returns a `Vec<&str>`
/// borrowing into the map for cheap iteration.
pub fn sorted_dates_desc(map: &Map<String, Value>) -> Vec<&str> {
    let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
    keys.sort_by(|a, b| b.cmp(a));
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn iso_date_detection() {
        assert!(is_iso_date("2024-01-31"));
        assert!(is_iso_date("1999-12-31"));
        assert!(!is_iso_date("2024-1-31")); // missing zero pad
        assert!(!is_iso_date("2024/01/31")); // wrong separator
        assert!(!is_iso_date(""));
        assert!(!is_iso_date("not-a-date"));
        assert!(!is_iso_date("2024-01-31T00:00")); // too long
    }

    #[test]
    fn periodic_map_recognised() {
        let m: Map<String, Value> = serde_json::from_value(json!({
            "2024-01-31": {"x": 1},
            "2024-04-30": {"x": 2},
        }))
        .unwrap();
        assert!(is_periodic_map(&m));

        let m2: Map<String, Value> = serde_json::from_value(json!({
            "2024-01-31": {"x": 1},
            "currency_symbol": "USD",
        }))
        .unwrap();
        assert!(!is_periodic_map(&m2));
    }

    #[test]
    fn last_n_keeps_newest() {
        let mut v = json!({
            "2023-01-01": "a",
            "2023-04-01": "b",
            "2023-07-01": "c",
            "2023-10-01": "d",
        });
        slice_periodic(&mut v, Some(2), None, None);
        let m = v.as_object().unwrap();
        assert_eq!(m.len(), 2);
        assert!(m.contains_key("2023-07-01"));
        assert!(m.contains_key("2023-10-01"));
    }

    #[test]
    fn date_range_filters_inclusive() {
        let mut v = json!({
            "2023-01-01": "a",
            "2023-04-01": "b",
            "2023-07-01": "c",
            "2023-10-01": "d",
        });
        slice_periodic(&mut v, None, Some("2023-04-01"), Some("2023-07-01"));
        let m = v.as_object().unwrap();
        assert_eq!(m.len(), 2);
        assert!(m.contains_key("2023-04-01"));
        assert!(m.contains_key("2023-07-01"));
    }

    #[test]
    fn slicing_recurses_into_nested_objects() {
        let mut v = json!({
            "Financials": {
                "Income_Statement": {
                    "currency_symbol": "USD",
                    "quarterly": {
                        "2023-01-01": {"rev": 1},
                        "2023-04-01": {"rev": 2},
                        "2023-07-01": {"rev": 3},
                    }
                }
            }
        });
        slice_periodic(&mut v, Some(1), None, None);
        let q = &v["Financials"]["Income_Statement"]["quarterly"];
        let m = q.as_object().unwrap();
        assert_eq!(m.len(), 1);
        assert!(m.contains_key("2023-07-01"));
        // currency_symbol siblings untouched
        assert_eq!(
            v["Financials"]["Income_Statement"]["currency_symbol"],
            "USD"
        );
    }

    #[test]
    fn empty_map_left_alone() {
        let mut v = json!({});
        slice_periodic(&mut v, Some(5), None, None);
        assert_eq!(v, json!({}));
    }

    #[test]
    fn no_op_when_no_params() {
        let mut v = json!({"2023-01-01": 1, "2023-04-01": 2});
        slice_periodic(&mut v, None, None, None);
        assert_eq!(v["2023-01-01"], 1);
        assert_eq!(v["2023-04-01"], 2);
    }

    #[test]
    fn as_f64_handles_string_and_number_and_null() {
        assert_eq!(as_f64(&json!(42)), Some(42.0));
        assert_eq!(as_f64(&json!(3.5)), Some(3.5));
        assert_eq!(as_f64(&json!("12345")), Some(12345.0));
        assert_eq!(as_f64(&json!("1.2e10")), Some(1.2e10));
        assert_eq!(as_f64(&json!(null)), None);
        assert_eq!(as_f64(&json!("not-a-number")), None);
    }
}
