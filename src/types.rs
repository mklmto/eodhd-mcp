use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SnapshotParams {
    /// Ticker symbol with exchange, e.g. "AAPL.US", "ALYA.TO"
    pub symbol: String,
    /// Optional reference date (YYYY-MM-DD) recorded in the response metadata.
    /// Does NOT alter what data is fetched — point-in-time queries are not yet
    /// supported. Defaults to today.
    pub as_of: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FinancialsToolParams {
    /// Ticker symbol with exchange, e.g. "AAPL.US"
    pub symbol: String,
    /// Statement to return: "income", "balance", "cashflow", or "all".
    pub statement: String,
    /// Period: "quarterly" (default) or "yearly".
    pub period: Option<String>,
    /// Number of most-recent periods to include. Default 8.
    pub last_n: Option<usize>,
    /// Reference date for the metadata block. Defaults to today.
    pub as_of: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CompareParams {
    /// Comma-separated ticker list (max 5), e.g. "ALYA.TO,GIB.TO,ACN.US".
    pub symbols: String,
    /// Comma-separated normalized metric keys, e.g.
    /// "ev_ebitda,ps,gross_margin,roe,net_debt_to_ebitda,fcf_yield,revenue_yoy".
    /// Available keys match the RatioSet field names plus growth metrics.
    pub metrics: String,
    /// Reference date for the metadata block. Defaults to today.
    pub as_of: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct HealthCheckParams {
    /// Ticker symbol with exchange, e.g. "AAPL.US"
    pub symbol: String,
    /// Reference date for the metadata block. Defaults to today.
    pub as_of: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchParams {
    /// Search query — ticker symbol, company name, or ISIN
    pub query: String,
    /// Exchange code filter (e.g. "US", "LSE", "FOREX"). Optional.
    pub exchange: Option<String>,
    /// Asset type filter: "stock", "etf", "fund", "bond", "index", "crypto". Optional.
    pub asset_type: Option<String>,
    /// Max results (default 15, max 500)
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct PriceParams {
    /// Ticker symbol with exchange, e.g. "AAPL.US", "BTC-USD.CC", "EURUSD.FOREX"
    pub symbol: String,
    /// Price data mode: "eod" (end-of-day historical), "intraday", or "realtime" (delayed live)
    pub mode: String,
    /// Start date (YYYY-MM-DD for eod; unix timestamp string for intraday). Optional.
    pub from: Option<String>,
    /// End date (YYYY-MM-DD for eod; unix timestamp string for intraday). Optional.
    pub to: Option<String>,
    /// EOD period: "d" (daily), "w" (weekly), "m" (monthly). Default "d". Only for eod mode.
    pub period: Option<String>,
    /// Intraday interval: "1m", "5m", "1h". Default "5m". Only for intraday mode.
    pub interval: Option<String>,
    /// Sort order: "a" (ascending) or "d" (descending). Optional.
    pub order: Option<String>,
    /// Additional comma-separated tickers for realtime mode. Optional.
    pub extra_symbols: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FundamentalsParams {
    /// Ticker symbol with exchange, e.g. "AAPL.US"
    pub symbol: String,
    /// Dot-separated filter path to return specific data, e.g. "General", "General::Code",
    /// "Financials::Balance_Sheet::yearly", "Highlights". Leave empty for full data.
    pub filter: Option<String>,
    /// Keep only the N most recent periods in any date-keyed periodic table found in
    /// the response (e.g. `Financials::*::quarterly`). Avoids returning 30+ quarters
    /// of historical noise. Applied after `from`/`to`. Optional.
    pub last_n: Option<usize>,
    /// Lower-bound date (YYYY-MM-DD, inclusive) for periodic tables. Optional.
    pub from: Option<String>,
    /// Upper-bound date (YYYY-MM-DD, inclusive) for periodic tables. Optional.
    pub to: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DividendsSplitsParams {
    /// Ticker symbol with exchange, e.g. "AAPL.US"
    pub symbol: String,
    /// Data type: "dividends" or "splits"
    pub data_type: String,
    /// Start date (YYYY-MM-DD). Optional.
    pub from: Option<String>,
    /// End date (YYYY-MM-DD). Optional.
    pub to: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct NewsParams {
    /// Ticker symbol (e.g. "AAPL.US"). Either symbol or topic must be provided.
    pub symbol: Option<String>,
    /// News topic tag (e.g. "technology", "earnings", "ipo", "mergers"). Either symbol or topic must be provided.
    pub topic: Option<String>,
    /// Start date (YYYY-MM-DD). Optional.
    pub from: Option<String>,
    /// End date (YYYY-MM-DD). Optional.
    pub to: Option<String>,
    /// Max results (default 50, max 1000). Optional.
    pub limit: Option<u32>,
    /// Pagination offset. Optional.
    pub offset: Option<u32>,
    /// If true, also fetches sentiment data for the given symbol. Only works when symbol is provided.
    pub include_sentiment: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TechnicalsParams {
    /// Ticker symbol with exchange, e.g. "AAPL.US"
    pub symbol: String,
    /// Technical indicator function: "sma", "ema", "wma", "rsi", "stochastic", "stochrsi",
    /// "macd", "volatility", "bbands", "atr", "stddev", "slope", "dmi", "adx", "sar",
    /// "avgvol", "avgvolccy", "cci", "beta", "splitadjusted"
    pub function: String,
    /// Calculation period (2-100000). Default 50. Optional.
    pub period: Option<u32>,
    /// Start date (YYYY-MM-DD). Optional.
    pub from: Option<String>,
    /// End date (YYYY-MM-DD). Optional.
    pub to: Option<String>,
    /// Sort order: "a" (ascending) or "d" (descending). Optional.
    pub order: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ScreenerParams {
    /// JSON-encoded filter array, e.g. '[["market_capitalization",">",1000000000],["sector","=","Technology"]]'.
    /// Fields: code, name, exchange, sector, industry, market_capitalization, earnings_share,
    /// dividend_yield, refund_1d_p, refund_5d_p, avgvol_1d, avgvol_200d, adjusted_close.
    pub filters: Option<String>,
    /// Comma-separated signal names: "200d_new_lo", "200d_new_hi", "bookvalue_neg",
    /// "bookvalue_pos", "wallstreet_lo", "wallstreet_hi". Optional.
    pub signals: Option<String>,
    /// Sort field and direction, e.g. "market_capitalization.desc". Optional.
    pub sort: Option<String>,
    /// Max results (default 50, max 100). Optional.
    pub limit: Option<u32>,
    /// Pagination offset (default 0, max 999). Optional.
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct MacroEconomicParams {
    /// Mode: "indicators" (macro indicator data) or "events" (economic events calendar)
    pub mode: String,
    /// ISO Alpha-3 country code for indicators (e.g. "USA", "GBR", "DEU") or
    /// ISO Alpha-2 for events (e.g. "US", "GB"). Required for indicators mode.
    pub country: Option<String>,
    /// Macro indicator name for indicators mode (e.g. "gdp_current_usd", "inflation_consumer_prices_annual",
    /// "unemployment_total_percent", "real_interest_rate", "population_total"). Default "gdp_current_usd".
    pub indicator: Option<String>,
    /// Start date (YYYY-MM-DD). Optional.
    pub from: Option<String>,
    /// End date (YYYY-MM-DD). Optional.
    pub to: Option<String>,
    /// Max results for events mode (default 50). Optional.
    pub limit: Option<u32>,
    /// Pagination offset for events mode. Optional.
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct InsiderTradingParams {
    /// Ticker symbol with exchange (e.g. "AAPL.US"). Leave empty for all tickers.
    pub symbol: Option<String>,
    /// Start date (YYYY-MM-DD). Optional.
    pub from: Option<String>,
    /// End date (YYYY-MM-DD). Optional.
    pub to: Option<String>,
    /// Max results (1-1000, default 100). Optional.
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CalendarParams {
    /// Calendar type: "earnings", "ipos", "splits", "dividends", "trends"
    pub calendar_type: String,
    /// Comma-separated ticker symbols. Optional (some calendar types support it).
    pub symbols: Option<String>,
    /// Start date (YYYY-MM-DD). Optional.
    pub from: Option<String>,
    /// End date (YYYY-MM-DD). Optional.
    pub to: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ExchangeInfoParams {
    /// Mode: "list" (all exchanges), "symbols" (tickers in an exchange), or "details" (exchange hours/holidays)
    pub mode: String,
    /// Exchange code (e.g. "US", "LSE", "CC", "FOREX"). Required for "symbols" and "details" modes.
    pub exchange: Option<String>,
    /// Asset type filter for symbols mode: "common_stock", "preferred_stock", "etf", "fund". Optional.
    pub asset_type: Option<String>,
    /// Include delisted tickers in symbols mode. Optional, default false.
    pub include_delisted: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct BulkDataParams {
    /// Exchange code, e.g. "US", "LSE"
    pub exchange: String,
    /// Data type: "eod" (default), "splits", or "dividends"
    pub data_type: Option<String>,
    /// Specific date (YYYY-MM-DD). Default: last trading day. Optional.
    pub date: Option<String>,
    /// Comma-separated symbols to filter (only for eod type). Optional.
    pub symbols: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TreasuryParams {
    /// Rate type: "bill" (T-bill), "long_term", "yield" (par yield curve), "real_yield"
    pub rate_type: String,
    /// Filter by year (e.g. 2024). Optional.
    pub year: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct MarketCapParams {
    /// Ticker symbol with exchange, e.g. "AAPL.US"
    pub symbol: String,
    /// Start date (YYYY-MM-DD). Optional.
    pub from: Option<String>,
    /// End date (YYYY-MM-DD). Optional.
    pub to: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct OptionsParams {
    /// Mode: "eod" (end-of-day option records with full Greeks: delta/gamma/theta/vega/rho,
    /// bid/ask/volume/OI — time series per contract), "contracts" (contract metadata only:
    /// strike/expiry/type — lighter than eod), or "underlyings" (list of ~6,000 covered US
    /// underlyings; ignores all filters).
    pub mode: String,

    /// Underlying ticker WITHOUT exchange suffix, e.g. "AAPL" (not "AAPL.US"). Filter for eod/contracts.
    pub underlying_symbol: Option<String>,

    /// OCC-format contract symbol, e.g. "AAPL250321C00150000". Filter for eod/contracts.
    pub contract: Option<String>,

    /// Option type: "call" or "put". Filter for eod/contracts.
    pub option_type: Option<String>,

    /// Minimum strike price (inclusive). Filter for eod/contracts.
    pub strike_from: Option<f64>,

    /// Maximum strike price (inclusive). Filter for eod/contracts.
    pub strike_to: Option<f64>,

    /// Earliest expiration date (YYYY-MM-DD). Filter for eod/contracts.
    pub exp_date_from: Option<String>,

    /// Latest expiration date (YYYY-MM-DD). Filter for eod/contracts.
    pub exp_date_to: Option<String>,

    /// Exact expiration date (YYYY-MM-DD). Filter for eod/contracts.
    pub exp_date_eq: Option<String>,

    /// Earliest trade date (YYYY-MM-DD). Filter for eod/contracts.
    pub tradetime_from: Option<String>,

    /// Latest trade date (YYYY-MM-DD). Filter for eod/contracts.
    pub tradetime_to: Option<String>,

    /// Expiration cycle type: "weekly", "monthly", "quarterly", etc. Filter for eod/contracts.
    pub expiration_type: Option<String>,

    /// Sort by field name (ascending). Prefix with "-" for descending,
    /// e.g. "-tradetime", "strike", "exp_date", "volume", "open_interest".
    pub sort: Option<String>,

    /// Pagination offset, default 0.
    pub page_offset: Option<u32>,

    /// Per-page record cap (default 1000, server max 1000 — values above 1000 are clamped).
    pub page_limit: Option<u32>,

    /// Sparse fieldset: comma-separated attribute names, e.g.
    /// "contract,bid,ask,delta,gamma,theta,vega,rho,tradetime".
    /// Sent as fields[options-eod] in eod mode and fields[options-contracts] in contracts mode.
    pub fields: Option<String>,

    /// If true, flatten the JSON:API envelope (drops type/attributes wrapping). Default false.
    pub compact: Option<bool>,

    /// If true, automatically follow links.next pagination links and merge data arrays.
    /// Default false. Capped by max_pages.
    pub auto_paginate: Option<bool>,

    /// Maximum pages fetched when auto_paginate is true. Default 5.
    pub max_pages: Option<u32>,
}
