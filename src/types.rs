use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
