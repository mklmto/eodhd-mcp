//! Cached fetchers shared by the new capability tools. Centralising the
//! cache keys here means `snapshot`, `financials`, and `health_check` all
//! benefit from each other's warm reads.

use crate::analytics::normalization::sorted_dates_desc;
use crate::cache::{Cache, CacheClass};
use crate::client::EodhdClient;
use serde_json::Value;

/// Fetch full fundamentals trimmed to the most recent `last_n` periods of
/// every periodic table. Cached at `fundamentals:{symbol}:last_n=N` for
/// 7 days (the spec's `fundamentals` TTL class).
pub async fn fundamentals_trimmed(
    client: &EodhdClient,
    cache: &Cache,
    symbol: &str,
    last_n: usize,
) -> Result<Value, String> {
    let last_n_str = last_n.to_string();
    let key = Cache::key(
        "fundamentals",
        &[("symbol", symbol), ("last_n", &last_n_str)],
    );
    cache
        .get_or_fetch(&key, CacheClass::Fundamentals.ttl(), || async {
            client
                .fundamentals_sliced(symbol, None, Some(last_n), None, None)
                .await
        })
        .await
}

/// Most recent `filing_date` (or `date`) found anywhere under
/// `Financials::*::quarterly`. Returns `None` if no quarterly data is
/// present (e.g. ETFs, indices). Used by snapshot freshness reporting.
pub fn most_recent_filing(fundamentals: &Value) -> Option<String> {
    let mut latest: Option<String> = None;
    for stmt in ["Income_Statement", "Balance_Sheet", "Cash_Flow"] {
        let q = match fundamentals
            .pointer(&format!("/Financials/{}/quarterly", stmt))
            .and_then(|v| v.as_object())
        {
            Some(q) => q,
            None => continue,
        };
        for d in sorted_dates_desc(q) {
            let entry = &q[d];
            let candidate = entry
                .get("filing_date")
                .and_then(|v| v.as_str())
                .unwrap_or(d);
            match &latest {
                Some(cur) if cur.as_str() >= candidate => {}
                _ => latest = Some(candidate.to_string()),
            }
            break; // only consider newest per statement
        }
    }
    latest
}
