# Changelog

All notable changes to `eodhd-mcp` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- **4 new capability tools** (`snapshot`, `financials`, `compare`, `health_check`) that compose multiple raw-API endpoints with a derived-analytics layer.
  - `snapshot` — one-call financial profile (CompanyInfo + Market + Profitability + Balance + CashFlow + Growth + Shareholder + DataFreshness + warnings). Replaces the 5-7 sequential `fundamentals` calls previously needed to assess a company.
  - `financials` — DataFrame-shaped statement view with derived margin / growth / TTM rows and a `TTM_4Q` column on quarterly views.
  - `compare` — side-by-side comparison across up to 5 tickers on 26 metric keys, with direction-aware ranking and parallel fetch.
  - `health_check` — 5-dimension scorecard (Profitability, Liquidity, Solvency, Efficiency, Growth), composite score, and red-flag list per spec Appendix C.
- **`src/analytics/` module:**
  - `ratios.rs` — 23 standard financial ratios (gross/operating/EBITDA/net margin, ROE, ROA, ROIC, current/quick/cash ratio, debt-to-equity, net debt / EBITDA, interest coverage, asset/inventory turnover, DSO, P/E, forward P/E, P/S, P/B, EV/Revenue, EV/EBITDA, FCF yield, PEG).
  - `ttm.rs` — strict 4-quarter TTM rollups + YoY growth.
  - `normalization.rs` — date detection, periodic slicing, null tolerance, string-or-number coercion.
  - `anomaly.rs` — Z-score outlier detection, CFO-vs-NI streak detector, revenue decline streak, negative-retained-with-buyback compound rule.
- **Persistent SQLite cache** (`src/cache/mod.rs`) with per-endpoint TTL classes:
  - 60 s for realtime / intraday
  - 24 h for EOD / market cap / bulk
  - 7 days for fundamentals / dividends / splits
  - 1 h for derived snapshot views
  - Configurable via `EODHD_CACHE_PATH` and `EODHD_CACHE_DISABLED` env vars.
- **`<summary>/<data>/<metadata>` output envelope** for all capability tools (spec §5.5) with prose summary, structured data block, freshness, warnings, sources, and cache-hit hint.
- **`fundamentals` tool** now accepts `last_n`, `from`, and `to` parameters that slice every date-keyed periodic table in the response — solves the spec's Problem #2 (no temporal filtering on fundamentals).
- **Comprehensive test suite** — 79 unit tests + 4 integration tests across analytics, cache, tools, and formatters. Includes an end-to-end ratio computation against a canned AAPL fundamentals fixture verifying every ratio against hand-computed values.
- **Validation harness** — `scripts/validate.ps1` and `scripts/validate.sh` drive the binary via stdio MCP against the spec Appendix B reference tickers (AAPL.US, ALYA.TO, SHOP.TO, GIB.TO, BRK-A.US).

### Changed
- `EodhdServer::new` now opens the default cache via `Cache::open_default()`. Cache failures are non-fatal (logged + degraded to direct API).
- Tool count: 16 → 20.

### Dependencies
- Added `rusqlite 0.32` (with `bundled` feature — no system SQLite needed).
- Added `chrono 0.4` (clock-only feature, no default features) for `as_of` date defaulting.

## [0.1.0] - 2026-02-20

Initial release with 16 raw EODHD API passthrough tools.
