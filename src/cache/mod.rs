//! Persistent SQLite cache for EODHD responses.
//!
//! - One row per `(endpoint, args)` key, value is the JSON response text.
//! - TTL is enforced read-side: an expired entry is treated as a miss and
//!   gets overwritten on the next refresh — we don't proactively delete.
//! - `Cache` is `Clone` (`Arc<Mutex<Connection>>` inside) so it can be held
//!   by `EodhdServer` without extra ceremony. The connection is serialised
//!   behind a `Mutex` because `rusqlite::Connection` is `!Sync`; SQLite ops
//!   are sub-millisecond so contention is negligible.
//! - All persistence operations are best-effort: a cache failure logs and
//!   degrades to a direct API call, never propagates an error.
//!
//! Disable entirely by setting `EODHD_CACHE_DISABLED=1`. Override the file
//! location with `EODHD_CACHE_PATH=/some/path/cache.db`.

use rusqlite::{params, Connection};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// TTL classes per spec §5.4. Variants are public API even when not all
/// are wired to a tool yet — `for_endpoint` documents the intended
/// classification table.
#[allow(dead_code)] // Realtime/Eod/Snapshot wired in by future tools; tests exercise them today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheClass {
    /// 60 s — real-time / delayed quotes.
    Realtime,
    /// 24 h — end-of-day prices, market cap.
    Eod,
    /// 7 days — fundamentals, dividends, splits.
    Fundamentals,
    /// 1 h — composed snapshot/financials views.
    Snapshot,
}

impl CacheClass {
    pub fn ttl(&self) -> Duration {
        match self {
            CacheClass::Realtime => Duration::from_secs(60),
            CacheClass::Eod => Duration::from_secs(24 * 3600),
            CacheClass::Fundamentals => Duration::from_secs(7 * 24 * 3600),
            CacheClass::Snapshot => Duration::from_secs(3600),
        }
    }

    /// Best-fit class for a known endpoint identifier. Conservative: if we
    /// don't recognise the endpoint, default to `Snapshot` (1 h) — short
    /// enough to avoid serving very stale data, long enough to be useful.
    #[allow(dead_code)] // generic helper — currently only `Fundamentals` used directly
    pub fn for_endpoint(endpoint: &str) -> Self {
        match endpoint {
            "realtime" | "intraday" => CacheClass::Realtime,
            "eod" | "market_cap" | "bulk_eod" => CacheClass::Eod,
            "fundamentals" | "dividends" | "splits" | "insider" => CacheClass::Fundamentals,
            _ => CacheClass::Snapshot,
        }
    }
}

#[derive(Clone)]
pub struct Cache {
    conn: Arc<Mutex<Connection>>,
    enabled: bool,
}

impl Cache {
    /// Open (or create) the cache at the configured path. Honours
    /// `EODHD_CACHE_DISABLED=1` and `EODHD_CACHE_PATH=/path/cache.db`.
    /// On any failure (filesystem, schema), returns a disabled cache —
    /// the server still functions, just without cache benefit.
    pub fn open_default() -> Self {
        if std::env::var("EODHD_CACHE_DISABLED")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            tracing::info!("EODHD cache disabled via EODHD_CACHE_DISABLED");
            return Self::disabled();
        }
        let path = Self::default_path();
        match Self::open(&path) {
            Ok(c) => {
                tracing::info!("EODHD cache opened at {}", path.display());
                c
            }
            Err(e) => {
                tracing::warn!(
                    "EODHD cache open failed at {} ({}); proceeding without cache",
                    path.display(),
                    e
                );
                Self::disabled()
            }
        }
    }

    fn default_path() -> PathBuf {
        if let Ok(p) = std::env::var("EODHD_CACHE_PATH") {
            return PathBuf::from(p);
        }
        // Next to the binary keeps everything self-contained.
        let mut p = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(std::env::temp_dir);
        p.push("eodhd-cache.db");
        p
    }

    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir parent: {}", e))?;
        }
        let conn = Connection::open(path).map_err(|e| format!("sqlite open: {}", e))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cache (
               key TEXT PRIMARY KEY,
               value TEXT NOT NULL,
               inserted_at INTEGER NOT NULL,
               ttl_seconds INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_cache_inserted_at ON cache(inserted_at);",
        )
        .map_err(|e| format!("sqlite schema: {}", e))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            enabled: true,
        })
    }

    pub fn disabled() -> Self {
        // A disabled cache still has an in-memory connection so the type
        // is uniform; we just never read or write through it.
        let conn = Connection::open_in_memory().expect("in-mem sqlite always opens");
        Self {
            conn: Arc::new(Mutex::new(conn)),
            enabled: false,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Build a stable cache key. `args` is sorted by key inside the helper
    /// so equivalent calls hash to the same key regardless of arg order.
    pub fn key(endpoint: &str, args: &[(&str, &str)]) -> String {
        let mut sorted: Vec<&(&str, &str)> = args.iter().collect();
        sorted.sort_by_key(|(k, _)| *k);
        let mut s = String::with_capacity(64);
        s.push_str(endpoint);
        s.push(':');
        for (k, v) in sorted {
            s.push_str(k);
            s.push('=');
            s.push_str(v);
            s.push('&');
        }
        s
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        if !self.enabled {
            return None;
        }
        let conn = self.conn.lock().ok()?;
        let mut stmt = conn
            .prepare("SELECT value, inserted_at, ttl_seconds FROM cache WHERE key = ?1")
            .ok()?;
        let row = stmt
            .query_row(params![key], |r| {
                let value: String = r.get(0)?;
                let inserted_at: i64 = r.get(1)?;
                let ttl: i64 = r.get(2)?;
                Ok((value, inserted_at, ttl))
            })
            .ok()?;
        let now = now_secs();
        if now - row.1 > row.2 {
            return None; // expired
        }
        serde_json::from_str(&row.0).ok()
    }

    pub fn put(&self, key: &str, value: &Value, ttl: Duration) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        let body = serde_json::to_string(value).map_err(|e| format!("cache serialize: {}", e))?;
        let conn = self.conn.lock().map_err(|e| format!("cache lock: {}", e))?;
        conn.execute(
            "INSERT OR REPLACE INTO cache (key, value, inserted_at, ttl_seconds)
             VALUES (?1, ?2, ?3, ?4)",
            params![key, body, now_secs(), ttl.as_secs() as i64],
        )
        .map_err(|e| format!("cache write: {}", e))?;
        Ok(())
    }

    #[allow(dead_code)] // housekeeping; intended for periodic invocation by future code
    pub fn purge_expired(&self) -> Result<usize, String> {
        if !self.enabled {
            return Ok(0);
        }
        let conn = self.conn.lock().map_err(|e| format!("cache lock: {}", e))?;
        let now = now_secs();
        let n = conn
            .execute(
                "DELETE FROM cache WHERE (?1 - inserted_at) > ttl_seconds",
                params![now],
            )
            .map_err(|e| format!("cache purge: {}", e))?;
        Ok(n)
    }

    /// Try the cache; on miss, run `fetch` and store the result.
    /// Cache failures are logged and never block the request.
    pub async fn get_or_fetch<F, Fut>(
        &self,
        key: &str,
        ttl: Duration,
        fetch: F,
    ) -> Result<Value, String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Value, String>>,
    {
        if let Some(hit) = self.get(key) {
            tracing::debug!(target: "eodhd_mcp", "cache hit: {}", key);
            return Ok(hit);
        }
        tracing::debug!(target: "eodhd_mcp", "cache miss: {}", key);
        let v = fetch().await?;
        if let Err(e) = self.put(key, &v, ttl) {
            tracing::warn!("cache put failed for {}: {}", key, e);
        }
        Ok(v)
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_cache() -> Cache {
        // Each test gets its own SQLite file — tests share a process so a
        // single per-PID path would let leftover state cross-contaminate.
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let mut p = std::env::temp_dir();
        p.push(format!("eodhd-cache-test-{}-{}.db", std::process::id(), n));
        let _ = std::fs::remove_file(&p);
        Cache::open(&p).expect("test cache opens")
    }

    #[test]
    fn ttl_classification_for_known_endpoints() {
        assert_eq!(CacheClass::for_endpoint("realtime"), CacheClass::Realtime);
        assert_eq!(CacheClass::for_endpoint("eod"), CacheClass::Eod);
        assert_eq!(
            CacheClass::for_endpoint("fundamentals"),
            CacheClass::Fundamentals
        );
        assert_eq!(CacheClass::for_endpoint("snapshot"), CacheClass::Snapshot);
        assert_eq!(CacheClass::for_endpoint("anything-else"), CacheClass::Snapshot);
    }

    #[test]
    fn key_is_arg_order_independent() {
        let a = Cache::key("fundamentals", &[("symbol", "AAPL.US"), ("filter", "General")]);
        let b = Cache::key("fundamentals", &[("filter", "General"), ("symbol", "AAPL.US")]);
        assert_eq!(a, b);
    }

    #[test]
    fn put_then_get_roundtrips() {
        let cache = temp_cache();
        let key = "roundtrip:test";
        cache
            .put(key, &json!({"x": 42}), Duration::from_secs(60))
            .unwrap();
        let v = cache.get(key).unwrap();
        assert_eq!(v["x"], 42);
    }

    #[test]
    fn expired_entries_treated_as_miss() {
        let cache = temp_cache();
        cache
            .put("ephemeral", &json!(1), Duration::from_secs(0))
            .unwrap();
        // ttl=0 means: any age > 0 → expired. Sleep 1s to be safe across coarse clocks.
        std::thread::sleep(Duration::from_millis(1100));
        assert!(cache.get("ephemeral").is_none());
    }

    #[test]
    fn disabled_cache_never_returns_anything() {
        let cache = Cache::disabled();
        assert!(!cache.is_enabled());
        cache
            .put("k", &json!(1), Duration::from_secs(60))
            .expect("put on disabled cache is no-op");
        assert!(cache.get("k").is_none());
    }

    #[tokio::test]
    async fn get_or_fetch_caches_on_miss_and_serves_on_hit() {
        let cache = temp_cache();
        let counter = std::sync::atomic::AtomicUsize::new(0);

        // First call → miss → fetch closure runs.
        let v1 = cache
            .get_or_fetch("composed", Duration::from_secs(60), || async {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok::<_, String>(json!({"v": "fresh"}))
            })
            .await
            .unwrap();
        assert_eq!(v1["v"], "fresh");

        // Second call → hit → fetch closure must NOT run again.
        let v2 = cache
            .get_or_fetch("composed", Duration::from_secs(60), || async {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok::<_, String>(json!({"v": "should-not-be-called"}))
            })
            .await
            .unwrap();
        assert_eq!(v2["v"], "fresh");
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn purge_removes_expired_only() {
        let cache = temp_cache();
        cache
            .put("alive", &json!("a"), Duration::from_secs(3600))
            .unwrap();
        cache
            .put("dead", &json!("b"), Duration::from_secs(0))
            .unwrap();
        std::thread::sleep(Duration::from_millis(1100));
        let removed = cache.purge_expired().unwrap();
        assert_eq!(removed, 1);
        assert!(cache.get("alive").is_some());
        assert!(cache.get("dead").is_none());
    }
}
