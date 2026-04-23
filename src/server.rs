use rmcp::{
    handler::server::tool::ToolRouter, handler::server::wrapper::Parameters, model::*, tool,
    tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};
use serde_json::Value;

use crate::cache::Cache;
use crate::client::EodhdClient;
use crate::format::format_value;
use crate::tools;
use crate::types::*;

/// Today's date as `YYYY-MM-DD` (UTC) — used as the default `as_of` for
/// new capability tools. Not a user-facing function; lives here so the
/// imports stay scoped to this module.
fn today_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

fn err(msg: String) -> McpError {
    McpError::internal_error(msg, None)
}

fn ok_text(text: String) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

#[derive(Clone)]
pub struct EodhdServer {
    client: EodhdClient,
    cache: Cache,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl EodhdServer {
    pub fn new(api_key: String) -> Self {
        let client = EodhdClient::new(api_key);
        let cache = Cache::open_default();
        Self {
            client,
            cache,
            tool_router: Self::tool_router(),
        }
    }

    // ── 1. Search ───────────────────────────────────────────────────────

    #[tool(
        name = "search",
        description = "Search for ticker symbols, companies, ETFs, funds, bonds, indices, or crypto by name, ticker, or ISIN. Returns matching instruments with their exchange codes. Note: demo API key does not support this endpoint."
    )]
    async fn search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let data = self
            .client
            .search(
                &params.query,
                params.exchange.as_deref(),
                params.asset_type.as_deref(),
                params.limit,
            )
            .await
            .map_err(err)?;
        ok_text(format_value(
            &format!("Search results for '{}'", params.query),
            &data,
        ))
    }

    // ── 2. Price ────────────────────────────────────────────────────────

    #[tool(
        name = "price",
        description = "Get stock/crypto/forex price data. Modes: 'eod' for end-of-day historical OHLCV (supports date range, daily/weekly/monthly periods), 'intraday' for intraday bars (1m/5m/1h intervals), 'realtime' for delayed live quotes (~15-20min delay for stocks, ~1min for forex). Symbol format: TICKER.EXCHANGE (e.g. AAPL.US, BTC-USD.CC, EURUSD.FOREX)."
    )]
    async fn price(
        &self,
        Parameters(params): Parameters<PriceParams>,
    ) -> Result<CallToolResult, McpError> {
        let data = match params.mode.as_str() {
            "eod" => {
                self.client
                    .eod(
                        &params.symbol,
                        params.from.as_deref(),
                        params.to.as_deref(),
                        params.period.as_deref(),
                        params.order.as_deref(),
                    )
                    .await
            }
            "intraday" => {
                self.client
                    .intraday(
                        &params.symbol,
                        params.interval.as_deref(),
                        params.from.as_deref(),
                        params.to.as_deref(),
                    )
                    .await
            }
            "realtime" => {
                self.client
                    .realtime(&params.symbol, params.extra_symbols.as_deref())
                    .await
            }
            other => Err(format!(
                "Invalid mode '{}'. Use: eod, intraday, realtime",
                other
            )),
        }
        .map_err(err)?;

        let label = format!(
            "{} price — {} ({})",
            params.symbol,
            params.mode,
            match params.mode.as_str() {
                "eod" => params.period.as_deref().unwrap_or("daily").to_string(),
                "intraday" => params.interval.as_deref().unwrap_or("5m").to_string(),
                "realtime" => "delayed".to_string(),
                _ => String::new(),
            }
        );
        ok_text(format_value(&label, &data))
    }

    // ── 3. Fundamentals ─────────────────────────────────────────────────

    #[tool(
        name = "fundamentals",
        description = "Get company fundamental data: general info, financials (income statement, balance sheet, cash flow), highlights, valuation, earnings, ESG scores, analyst ratings, and more. Use the 'filter' parameter to extract specific sections (e.g. 'General', 'Highlights', 'Financials::Balance_Sheet::yearly', 'Valuation'). Use 'last_n' to keep only the N most recent quarters/years from any date-keyed periodic table (avoids returning 30+ quarters of historical noise). 'from' and 'to' (YYYY-MM-DD) further restrict the date range. Without filter returns full data — combine with 'last_n' to keep response size manageable. Works for stocks, ETFs, mutual funds, and indices."
    )]
    async fn fundamentals(
        &self,
        Parameters(params): Parameters<FundamentalsParams>,
    ) -> Result<CallToolResult, McpError> {
        let data = self
            .client
            .fundamentals_sliced(
                &params.symbol,
                params.filter.as_deref(),
                params.last_n,
                params.from.as_deref(),
                params.to.as_deref(),
            )
            .await
            .map_err(err)?;
        let mut label = match &params.filter {
            Some(f) => format!("{} fundamentals ({})", params.symbol, f),
            None => format!("{} fundamentals (full)", params.symbol),
        };
        if let Some(n) = params.last_n {
            label.push_str(&format!(", last {}", n));
        }
        if params.from.is_some() || params.to.is_some() {
            label.push_str(&format!(
                ", range {}..{}",
                params.from.as_deref().unwrap_or(""),
                params.to.as_deref().unwrap_or("")
            ));
        }
        ok_text(format_value(&label, &data))
    }

    // ── 4. Dividends & Splits ───────────────────────────────────────────

    #[tool(
        name = "dividends_splits",
        description = "Get historical dividends or stock splits data. Set data_type to 'dividends' for dividend payment history or 'splits' for stock split history."
    )]
    async fn dividends_splits(
        &self,
        Parameters(params): Parameters<DividendsSplitsParams>,
    ) -> Result<CallToolResult, McpError> {
        let data = match params.data_type.as_str() {
            "dividends" => {
                self.client
                    .dividends(&params.symbol, params.from.as_deref(), params.to.as_deref())
                    .await
            }
            "splits" => {
                self.client
                    .splits(&params.symbol, params.from.as_deref(), params.to.as_deref())
                    .await
            }
            other => Err(format!(
                "Invalid data_type '{}'. Use: dividends, splits",
                other
            )),
        }
        .map_err(err)?;

        ok_text(format_value(
            &format!("{} {} history", params.symbol, params.data_type),
            &data,
        ))
    }

    // ── 5. News & Sentiment ─────────────────────────────────────────────

    #[tool(
        name = "news",
        description = "Get financial news articles and optionally sentiment data. Provide either a symbol (e.g. 'AAPL.US') or a topic tag (e.g. 'technology', 'earnings', 'ipo', 'mergers'). Set include_sentiment=true to also fetch sentiment scores (-1 to +1) for the symbol."
    )]
    async fn news(
        &self,
        Parameters(params): Parameters<NewsParams>,
    ) -> Result<CallToolResult, McpError> {
        if params.symbol.is_none() && params.topic.is_none() {
            return Err(err("Either 'symbol' or 'topic' must be provided.".into()));
        }

        let news_data = self
            .client
            .news(
                params.symbol.as_deref(),
                params.topic.as_deref(),
                params.from.as_deref(),
                params.to.as_deref(),
                params.limit,
                params.offset,
            )
            .await
            .map_err(err)?;

        let mut out = format_value("Financial News", &news_data);

        if params.include_sentiment.unwrap_or(false) {
            if let Some(ref sym) = params.symbol {
                match self
                    .client
                    .sentiment(sym, params.from.as_deref(), params.to.as_deref())
                    .await
                {
                    Ok(sent) => {
                        out.push_str("\n\n---\n\n");
                        out.push_str(&format_value(&format!("Sentiment for {}", sym), &sent));
                    }
                    Err(e) => {
                        out.push_str(&format!("\n\n*Sentiment data unavailable: {}*\n", e));
                    }
                }
            }
        }

        ok_text(out)
    }

    // ── 6. Technical Indicators ─────────────────────────────────────────

    #[tool(
        name = "technicals",
        description = "Calculate technical indicators for a symbol. Functions: sma, ema, wma, rsi, stochastic, stochrsi, macd, volatility, bbands (Bollinger Bands), atr, stddev, slope, dmi, adx, sar (Parabolic SAR), avgvol, avgvolccy, cci, beta, splitadjusted. Period controls the lookback window (default 50)."
    )]
    async fn technicals(
        &self,
        Parameters(params): Parameters<TechnicalsParams>,
    ) -> Result<CallToolResult, McpError> {
        let data = self
            .client
            .technicals(
                &params.symbol,
                &params.function,
                params.period,
                params.from.as_deref(),
                params.to.as_deref(),
                params.order.as_deref(),
            )
            .await
            .map_err(err)?;

        ok_text(format_value(
            &format!(
                "{} — {} (period {})",
                params.symbol,
                params.function.to_uppercase(),
                params.period.unwrap_or(50)
            ),
            &data,
        ))
    }

    // ── 7. Stock Screener ───────────────────────────────────────────────

    #[tool(
        name = "screener",
        description = "Screen stocks by financial criteria. Filters use JSON array format: '[[\"market_capitalization\",\">\",1000000000],[\"sector\",\"=\",\"Technology\"]]'. Available fields: code, name, exchange, sector, industry, market_capitalization, earnings_share, dividend_yield, refund_1d_p, refund_5d_p, avgvol_1d, avgvol_200d, adjusted_close. Signals: 200d_new_lo, 200d_new_hi, bookvalue_neg, bookvalue_pos, wallstreet_lo, wallstreet_hi."
    )]
    async fn screener(
        &self,
        Parameters(params): Parameters<ScreenerParams>,
    ) -> Result<CallToolResult, McpError> {
        let data = self
            .client
            .screener(
                params.filters.as_deref(),
                params.signals.as_deref(),
                params.sort.as_deref(),
                params.limit,
                params.offset,
            )
            .await
            .map_err(err)?;

        ok_text(format_value("Stock Screener Results", &data))
    }

    // ── 8. Macro Economic ───────────────────────────────────────────────

    #[tool(
        name = "macro_economic",
        description = "Get macroeconomic data. Mode 'indicators': country-level macro indicators (GDP, inflation, unemployment, etc.) using ISO Alpha-3 country code. Mode 'events': upcoming/past economic events calendar. Indicators include: gdp_current_usd, gdp_per_capita_usd, gdp_growth_annual, inflation_consumer_prices_annual, unemployment_total_percent, real_interest_rate, population_total, life_expectancy, debt_percent_gdp, and many more."
    )]
    async fn macro_economic(
        &self,
        Parameters(params): Parameters<MacroEconomicParams>,
    ) -> Result<CallToolResult, McpError> {
        let data = match params.mode.as_str() {
            "indicators" => {
                let country = params.country.as_deref().ok_or_else(|| {
                    err(
                        "'country' is required for indicators mode (ISO Alpha-3, e.g. 'USA')"
                            .into(),
                    )
                })?;
                self.client
                    .macro_indicator(country, params.indicator.as_deref())
                    .await
            }
            "events" => {
                self.client
                    .economic_events(
                        params.from.as_deref(),
                        params.to.as_deref(),
                        params.country.as_deref(),
                        params.limit,
                        params.offset,
                    )
                    .await
            }
            other => Err(format!("Invalid mode '{}'. Use: indicators, events", other)),
        }
        .map_err(err)?;

        let label = match params.mode.as_str() {
            "indicators" => format!(
                "Macro Indicator: {} — {}",
                params.country.as_deref().unwrap_or("?"),
                params.indicator.as_deref().unwrap_or("gdp_current_usd")
            ),
            _ => "Economic Events".to_string(),
        };
        ok_text(format_value(&label, &data))
    }

    // ── 9. Insider Trading ──────────────────────────────────────────────

    #[tool(
        name = "insider_trading",
        description = "Get insider transactions (SEC Form 4 filings). Shows insider buys, sells, and exercises for US stocks. Can filter by symbol or get all recent filings."
    )]
    async fn insider_trading(
        &self,
        Parameters(params): Parameters<InsiderTradingParams>,
    ) -> Result<CallToolResult, McpError> {
        let data = self
            .client
            .insider_transactions(
                params.symbol.as_deref(),
                params.from.as_deref(),
                params.to.as_deref(),
                params.limit,
            )
            .await
            .map_err(err)?;

        let label = match &params.symbol {
            Some(s) => format!("Insider Transactions — {}", s),
            None => "Recent Insider Transactions".to_string(),
        };
        ok_text(format_value(&label, &data))
    }

    // ── 10. Calendar ────────────────────────────────────────────────────

    #[tool(
        name = "calendar",
        description = "Get financial calendar data. Types: 'earnings' (earnings release dates and EPS estimates), 'ipos' (upcoming/recent IPOs), 'splits' (stock split events), 'dividends' (dividend ex-dates and amounts), 'trends' (earnings trend estimates — requires symbols)."
    )]
    async fn calendar(
        &self,
        Parameters(params): Parameters<CalendarParams>,
    ) -> Result<CallToolResult, McpError> {
        let valid = ["earnings", "ipos", "splits", "dividends", "trends"];
        if !valid.contains(&params.calendar_type.as_str()) {
            return Err(err(format!(
                "Invalid calendar_type '{}'. Use: {}",
                params.calendar_type,
                valid.join(", ")
            )));
        }

        let data = self
            .client
            .calendar(
                &params.calendar_type,
                params.symbols.as_deref(),
                params.from.as_deref(),
                params.to.as_deref(),
            )
            .await
            .map_err(err)?;

        ok_text(format_value(
            &format!("{} Calendar", capitalize(&params.calendar_type)),
            &data,
        ))
    }

    // ── 11. Exchange Info ───────────────────────────────────────────────

    #[tool(
        name = "exchange_info",
        description = "Get exchange information. Modes: 'list' (all available exchanges), 'symbols' (all tickers on an exchange — requires exchange code), 'details' (trading hours, holidays for an exchange). Exchange codes include: US, LSE, TO, HK, F, PA, AS, CC (crypto), FOREX, INDX, GBOND, etc."
    )]
    async fn exchange_info(
        &self,
        Parameters(params): Parameters<ExchangeInfoParams>,
    ) -> Result<CallToolResult, McpError> {
        let data = match params.mode.as_str() {
            "list" => self.client.exchanges_list().await,
            "symbols" => {
                let ex = params
                    .exchange
                    .as_deref()
                    .ok_or_else(|| err("'exchange' is required for symbols mode".into()))?;
                self.client
                    .exchange_symbols(
                        ex,
                        params.asset_type.as_deref(),
                        params.include_delisted.unwrap_or(false),
                    )
                    .await
            }
            "details" => {
                let ex = params
                    .exchange
                    .as_deref()
                    .ok_or_else(|| err("'exchange' is required for details mode".into()))?;
                self.client.exchange_details(ex, None, None).await
            }
            other => Err(format!(
                "Invalid mode '{}'. Use: list, symbols, details",
                other
            )),
        }
        .map_err(err)?;

        let label = match params.mode.as_str() {
            "list" => "Available Exchanges".to_string(),
            "symbols" => format!("Symbols on {}", params.exchange.as_deref().unwrap_or("?")),
            "details" => format!(
                "Exchange Details — {}",
                params.exchange.as_deref().unwrap_or("?")
            ),
            _ => "Exchange Info".to_string(),
        };
        ok_text(format_value(&label, &data))
    }

    // ── 12. Bulk Data ───────────────────────────────────────────────────

    #[tool(
        name = "bulk_data",
        description = "Get bulk end-of-day data for an entire exchange in a single call. Data types: 'eod' (OHLCV for all tickers), 'splits', 'dividends'. Can filter to specific symbols. Costs 100 API calls per request."
    )]
    async fn bulk_data(
        &self,
        Parameters(params): Parameters<BulkDataParams>,
    ) -> Result<CallToolResult, McpError> {
        let data = self
            .client
            .bulk_eod(
                &params.exchange,
                params.data_type.as_deref(),
                params.date.as_deref(),
                params.symbols.as_deref(),
            )
            .await
            .map_err(err)?;

        ok_text(format_value(
            &format!(
                "Bulk {} data — {}",
                params.data_type.as_deref().unwrap_or("eod"),
                params.exchange
            ),
            &data,
        ))
    }

    // ── 13. Treasury Rates ──────────────────────────────────────────────

    #[tool(
        name = "treasury",
        description = "Get US Treasury interest rate data. Types: 'bill' (T-bill rates), 'long_term' (long-term rates), 'yield' (par yield curve rates), 'real_yield' (real yield rates). Can filter by year."
    )]
    async fn treasury(
        &self,
        Parameters(params): Parameters<TreasuryParams>,
    ) -> Result<CallToolResult, McpError> {
        let data = self
            .client
            .treasury_rates(&params.rate_type, params.year)
            .await
            .map_err(err)?;

        ok_text(format_value(
            &format!(
                "US Treasury — {}{}",
                params.rate_type,
                params.year.map(|y| format!(" ({})", y)).unwrap_or_default()
            ),
            &data,
        ))
    }

    // ── 14. Historical Market Cap ───────────────────────────────────────

    #[tool(
        name = "market_cap",
        description = "Get historical market capitalization data for a US stock. Weekly granularity, available from 2020 onwards."
    )]
    async fn market_cap(
        &self,
        Parameters(params): Parameters<MarketCapParams>,
    ) -> Result<CallToolResult, McpError> {
        let data = self
            .client
            .historical_market_cap(&params.symbol, params.from.as_deref(), params.to.as_deref())
            .await
            .map_err(err)?;

        ok_text(format_value(
            &format!("{} Historical Market Cap", params.symbol),
            &data,
        ))
    }

    // ── 15. Account Usage ───────────────────────────────────────────────

    #[tool(
        name = "account",
        description = "Get your EODHD API account information: subscription type, API call usage, daily rate limit, and remaining quota."
    )]
    async fn account(&self) -> Result<CallToolResult, McpError> {
        let data = self.client.user().await.map_err(err)?;
        ok_text(format_value("EODHD Account", &data))
    }

    // ── 17. Snapshot (capability tool) ──────────────────────────────────

    #[tool(
        name = "snapshot",
        description = "One-call financial profile composing General + Highlights + Valuation + SharesStats + Financials::* with derived analytics (TTM margins, leverage, FCF yield, YoY growth, freshness, warnings). Replaces the 5-7 fundamentals calls previously needed to assess a company. Returns the spec §5.5 envelope: <summary> prose / <data> structured JSON / <metadata>. Cached 7 days under the fundamentals TTL class. Symbol format: TICKER.EXCHANGE."
    )]
    async fn snapshot(
        &self,
        Parameters(params): Parameters<SnapshotParams>,
    ) -> Result<CallToolResult, McpError> {
        let as_of = params.as_of.unwrap_or_else(today_iso);
        let body = tools::snapshot::run(&self.client, &self.cache, &params.symbol, &as_of)
            .await
            .map_err(err)?;
        ok_text(body)
    }

    // ── 18. Financials (DataFrame view) ─────────────────────────────────

    #[tool(
        name = "financials",
        description = "DataFrame-shaped financial statements with derived rows. Statement: 'income', 'balance', 'cashflow', or 'all'. Period: 'quarterly' (default) or 'yearly'. last_n (default 8) caps period count. Output is a markdown table per statement: rows = native EODHD line items + derived rows (margins %, QoQ revenue growth, FCF, net debt) + a TTM_4Q column on quarterly views. Wrapped in the spec §5.5 envelope. Shares the fundamentals cache with snapshot/health_check (7-day TTL)."
    )]
    async fn financials(
        &self,
        Parameters(params): Parameters<FinancialsToolParams>,
    ) -> Result<CallToolResult, McpError> {
        let opts = tools::financials::Options::new(
            &params.statement,
            params.period.as_deref(),
            params.last_n,
        )
        .map_err(err)?;
        let as_of = params.as_of.unwrap_or_else(today_iso);
        let body = tools::financials::run(&self.client, &self.cache, &params.symbol, opts, &as_of)
            .await
            .map_err(err)?;
        ok_text(body)
    }

    // ── 19. Compare (multi-ticker) ──────────────────────────────────────

    #[tool(
        name = "compare",
        description = "Side-by-side metric comparison across up to 5 tickers. 'symbols' is comma-separated (e.g. 'ALYA.TO,GIB.TO,ACN.US'); 'metrics' is comma-separated metric keys (e.g. 'ev_ebitda,ps,gross_margin,roe,net_debt_to_ebitda,fcf_yield,revenue_yoy'). Available keys cover profitability (gross/operating/ebitda/net margin, roe, roa, roic), liquidity (current/quick/cash ratio), solvency (debt_to_equity, net_debt_to_ebitda, interest_coverage), efficiency (asset/inventory turnover, dso), valuation (pe, forward_pe, ps, pb, ev_revenue, ev_ebitda, fcf_yield, peg), and growth (revenue_yoy, net_income_yoy). Returns a values table and a ranking table (1=best per metric, direction-aware: lower wins for debt/valuation, higher wins for margins/growth). Wrapped in the spec §5.5 envelope. Fetches in parallel against the shared fundamentals cache."
    )]
    async fn compare(
        &self,
        Parameters(params): Parameters<CompareParams>,
    ) -> Result<CallToolResult, McpError> {
        let opts = tools::compare::Options::parse(&params.symbols, &params.metrics).map_err(err)?;
        let as_of = params.as_of.unwrap_or_else(today_iso);
        let body = tools::compare::run(&self.client, &self.cache, opts, &as_of)
            .await
            .map_err(err)?;
        ok_text(body)
    }

    // ── 20. Health Check (scorecard) ────────────────────────────────────

    #[tool(
        name = "health_check",
        description = "Five-dimension financial health scorecard. Computes 0-100 scores for Profitability, Liquidity, Solvency, Efficiency, and Growth based on band thresholds, plus a composite. Surfaces red flags per spec Appendix C: net debt / EBITDA > 3×, interest coverage < 3×, revenue YoY < 0 for 3 consecutive quarters, CFO < net income for 3 consecutive quarters (earnings quality), negative retained earnings + active buyback, Z-score outliers on otherNonCashItems (proxy for non-recurring charges), and gross margin > 2σ deviation. Wrapped in spec §5.5 envelope. Reuses the shared fundamentals cache."
    )]
    async fn health_check(
        &self,
        Parameters(params): Parameters<HealthCheckParams>,
    ) -> Result<CallToolResult, McpError> {
        let as_of = params.as_of.unwrap_or_else(today_iso);
        let body = tools::health_check::run(&self.client, &self.cache, &params.symbol, &as_of)
            .await
            .map_err(err)?;
        ok_text(body)
    }

    // ── 16. US Options (Unicornbay) ─────────────────────────────────────

    #[tool(
        name = "options",
        description = "Query the EODHD Unicornbay US Stock Options dataset (~6,000 underlyings, 2-year history, NASDAQ-routed). Modes: 'eod' (end-of-day option records with full Greeks — delta/gamma/theta/vega/rho — and bid/ask/volume/open_interest as a per-contract time series), 'contracts' (lighter contract-discovery metadata: strike/expiry/type), 'underlyings' (list of covered tickers; takes no filters). Filters: underlying_symbol (no exchange suffix), contract (OCC format), option_type (call/put), strike_from/to, exp_date_from/to/eq, tradetime_from/to, expiration_type. Pagination: page_offset/page_limit (server max 1000); set auto_paginate=true to follow links.next up to max_pages and merge the data arrays. Use the 'fields' parameter for sparse fieldsets and 'compact' to flatten the JSON:API envelope. Requires an active EODHD marketplace Options subscription on the account."
    )]
    async fn options(
        &self,
        Parameters(params): Parameters<OptionsParams>,
    ) -> Result<CallToolResult, McpError> {
        let valid_modes = ["eod", "contracts", "underlyings"];
        if !valid_modes.contains(&params.mode.as_str()) {
            return Err(err(format!(
                "Invalid mode '{}'. Use: eod, contracts, underlyings",
                params.mode
            )));
        }

        // Reject filters on underlyings mode (server ignores them; fail fast).
        if params.mode == "underlyings" && has_any_filter(&params) {
            return Err(err(
                "'underlyings' mode does not accept filters. Remove all filter parameters or switch to 'eod' / 'contracts'.".into()
            ));
        }

        let mut notes: Vec<String> = Vec::new();

        // Warn (non-fatal) if eod/contracts has no narrowing filter.
        if (params.mode == "eod" || params.mode == "contracts")
            && params.underlying_symbol.is_none()
            && params.contract.is_none()
        {
            notes.push(
                "no underlying_symbol or contract filter set — this query will scan the entire dataset and can exhaust your daily API quota. Add a filter to narrow the result.".into()
            );
        }

        if let Some(lim) = params.page_limit {
            if lim > 1000 {
                notes.push(format!(
                    "page_limit {} exceeds the server max of 1000 — clamped to 1000.",
                    lim
                ));
            }
        }

        let query = build_options_query(&params);
        let first = self
            .client
            .options_query(&params.mode, &query)
            .await
            .map_err(|e| err(scrub_token(&e)))?;

        let auto = params.auto_paginate.unwrap_or(false);
        let max_pages = params.max_pages.unwrap_or(5).max(1);

        let merged = if auto {
            let mut combined: Vec<Value> = first
                .get("data")
                .and_then(|d| d.as_array())
                .cloned()
                .unwrap_or_default();
            let mut meta = first
                .get("meta")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let mut next = first
                .get("links")
                .and_then(|l| l.get("next"))
                .and_then(|n| n.as_str())
                .map(|s| s.to_string());
            let mut pages_fetched: u32 = 1;

            while let Some(url) = next.clone() {
                if pages_fetched >= max_pages {
                    break;
                }
                let scrubbed = scrub_token(&url);
                tracing::debug!(target: "eodhd_mcp", "options auto-paginate: fetching next page {}", scrubbed);
                let page = self
                    .client
                    .options_follow_url(&url)
                    .await
                    .map_err(|e| err(scrub_token(&e)))?;
                if let Some(arr) = page.get("data").and_then(|d| d.as_array()) {
                    combined.extend(arr.iter().cloned());
                }
                if let Some(m) = page.get("meta").cloned() {
                    meta = m;
                }
                next = page
                    .get("links")
                    .and_then(|l| l.get("next"))
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string());
                pages_fetched += 1;
            }

            if let Value::Object(ref mut m) = meta {
                m.insert("pages_fetched".into(), serde_json::json!(pages_fetched));
                if next.is_some() {
                    m.insert("pagination_cap_hit".into(), serde_json::json!(true));
                }
            }

            let scrubbed_next: Value = next
                .as_deref()
                .map(|u| Value::String(scrub_token(u)))
                .unwrap_or(Value::Null);

            serde_json::json!({
                "meta": meta,
                "data": combined,
                "links": { "next": scrubbed_next },
            })
        } else {
            let mut v = first;
            if let Some(links) = v.get_mut("links") {
                if let Some(next) = links.get_mut("next") {
                    if let Some(s) = next.as_str() {
                        *next = Value::String(scrub_token(s));
                    }
                }
            }
            v
        };

        Ok(CallToolResult::success(vec![Content::text(
            render_options_response(&params.mode, &merged, &notes),
        )]))
    }
}

#[tool_handler]
impl ServerHandler for EodhdServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "EODHD Financial Data MCP Server — provides access to end-of-day prices, \
                 intraday data, real-time quotes, company fundamentals, financial news, \
                 technical indicators, stock screening, macro economic data, insider \
                 transactions, calendar events, exchange information, and more via the \
                 EODHD API. Symbol format: TICKER.EXCHANGE (e.g. AAPL.US, BTC-USD.CC)."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

// ── Unicornbay Options helpers ──────────────────────────────────────────

fn has_any_filter(p: &OptionsParams) -> bool {
    p.underlying_symbol.is_some()
        || p.contract.is_some()
        || p.option_type.is_some()
        || p.strike_from.is_some()
        || p.strike_to.is_some()
        || p.exp_date_from.is_some()
        || p.exp_date_to.is_some()
        || p.exp_date_eq.is_some()
        || p.tradetime_from.is_some()
        || p.tradetime_to.is_some()
        || p.expiration_type.is_some()
}

fn format_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e16 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

fn build_options_query(p: &OptionsParams) -> Vec<(String, String)> {
    let mut q: Vec<(String, String)> = Vec::new();

    // underlyings mode takes no query parameters.
    if p.mode == "underlyings" {
        return q;
    }

    if let Some(v) = &p.underlying_symbol {
        q.push(("filter[underlying_symbol]".into(), v.clone()));
    }
    if let Some(v) = &p.contract {
        q.push(("filter[contract]".into(), v.clone()));
    }
    if let Some(v) = &p.option_type {
        q.push(("filter[type]".into(), v.clone()));
    }
    if let Some(v) = p.strike_from {
        q.push(("filter[strike_from]".into(), format_number(v)));
    }
    if let Some(v) = p.strike_to {
        q.push(("filter[strike_to]".into(), format_number(v)));
    }
    if let Some(v) = &p.exp_date_from {
        q.push(("filter[exp_date_from]".into(), v.clone()));
    }
    if let Some(v) = &p.exp_date_to {
        q.push(("filter[exp_date_to]".into(), v.clone()));
    }
    if let Some(v) = &p.exp_date_eq {
        q.push(("filter[exp_date_eq]".into(), v.clone()));
    }
    if let Some(v) = &p.tradetime_from {
        q.push(("filter[tradetime_from]".into(), v.clone()));
    }
    if let Some(v) = &p.tradetime_to {
        q.push(("filter[tradetime_to]".into(), v.clone()));
    }
    if let Some(v) = &p.expiration_type {
        q.push(("filter[expiration_type]".into(), v.clone()));
    }

    if let Some(v) = &p.sort {
        q.push(("sort".into(), v.clone()));
    }

    if let Some(off) = p.page_offset {
        q.push(("page[offset]".into(), off.to_string()));
    }
    if let Some(lim) = p.page_limit {
        q.push(("page[limit]".into(), lim.min(1000).to_string()));
    }

    if let Some(v) = &p.fields {
        let key = match p.mode.as_str() {
            "eod" => Some("fields[options-eod]"),
            "contracts" => Some("fields[options-contracts]"),
            _ => None,
        };
        if let Some(k) = key {
            q.push((k.into(), v.clone()));
        }
    }

    if p.compact.unwrap_or(false) {
        q.push(("compact".into(), "1".into()));
    }

    q
}

/// Replace any `api_token=VALUE` occurrences with `api_token=***` so the user's key
/// never ends up in logs, error messages, or surfaced pagination URLs. The token
/// value is considered to run until the first non-token character (URL-safe chars:
/// alphanumerics, `-`, `.`, `_`, `~`, `%`).
fn scrub_token(s: &str) -> String {
    let pattern = "api_token=";
    let mut result = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(idx) = rest.find(pattern) {
        let head_end = idx + pattern.len();
        result.push_str(&rest[..head_end]);
        result.push_str("***");
        let after = &rest[head_end..];
        let mut end = 0;
        for (i, c) in after.char_indices() {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_' || c == '~' || c == '%'
            {
                end = i + c.len_utf8();
            } else {
                break;
            }
        }
        rest = &after[end..];
    }
    result.push_str(rest);
    result
}

/// Render a Unicornbay options response. For non-compact JSON:API records the
/// `attributes` map is hoisted into a flat row so the standard table formatter
/// produces a useful display; otherwise the response is dumped as JSON.
fn render_options_response(mode: &str, body: &Value, notes: &[String]) -> String {
    let mut out = String::new();
    for n in notes {
        out.push_str(&format!("> **Note:** {}\n\n", n));
    }

    let label = match mode {
        "eod" => "Options EOD",
        "contracts" => "Options Contracts",
        "underlyings" => "Options Underlyings",
        _ => "Options",
    };

    let data = body.get("data");
    let meta = body.get("meta");
    let next = body
        .get("links")
        .and_then(|l| l.get("next"))
        .filter(|v| !v.is_null())
        .and_then(|v| v.as_str());

    let total = meta.and_then(|m| m.get("total")).and_then(|v| v.as_u64());
    let pages_fetched = meta
        .and_then(|m| m.get("pages_fetched"))
        .and_then(|v| v.as_u64());
    let cap_hit = meta
        .and_then(|m| m.get("pagination_cap_hit"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let display = data.and_then(|d| d.as_array()).map(|arr| {
        let rows: Vec<Value> = arr.iter().map(flatten_jsonapi).collect();
        Value::Array(rows)
    });

    let mut header = label.to_string();
    if let Some(t) = total {
        header.push_str(&format!(" — total {}", t));
    }
    if let Some(p) = pages_fetched {
        header.push_str(&format!(", pages fetched {}", p));
    }
    if cap_hit {
        header.push_str(" (max_pages cap hit)");
    }

    match display {
        Some(arr) => out.push_str(&format_value(&header, &arr)),
        None => out.push_str(&format_value(&header, body)),
    }

    if let Some(url) = next {
        out.push_str(&format!("\n\n**links.next:** `{}`\n", url));
    }

    out
}

/// If `item` is a JSON:API record `{id, type, attributes: {...}}`, hoist the
/// attributes into a flat object (preserving id under `_id`). Otherwise return as-is.
fn flatten_jsonapi(item: &Value) -> Value {
    if let Some(attrs) = item.get("attributes").and_then(|a| a.as_object()) {
        let mut flat = attrs.clone();
        if let Some(id) = item.get("id") {
            flat.insert("_id".into(), id.clone());
        }
        Value::Object(flat)
    } else {
        item.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::OptionsParams;

    fn empty_params(mode: &str) -> OptionsParams {
        OptionsParams {
            mode: mode.into(),
            underlying_symbol: None,
            contract: None,
            option_type: None,
            strike_from: None,
            strike_to: None,
            exp_date_from: None,
            exp_date_to: None,
            exp_date_eq: None,
            tradetime_from: None,
            tradetime_to: None,
            expiration_type: None,
            sort: None,
            page_offset: None,
            page_limit: None,
            fields: None,
            compact: None,
            auto_paginate: None,
            max_pages: None,
        }
    }

    #[test]
    fn underlyings_mode_emits_no_query() {
        let mut p = empty_params("underlyings");
        // Filters set on this mode are dropped by the builder (the tool itself rejects them).
        p.underlying_symbol = Some("AAPL".into());
        assert!(build_options_query(&p).is_empty());
    }

    #[test]
    fn all_eod_filters_serialize_to_jsonapi_brackets() {
        let mut p = empty_params("eod");
        p.underlying_symbol = Some("AAPL".into());
        p.contract = Some("AAPL250321C00150000".into());
        p.option_type = Some("call".into());
        p.strike_from = Some(150.0);
        p.strike_to = Some(200.5);
        p.exp_date_from = Some("2024-01-21".into());
        p.exp_date_to = Some("2024-01-28".into());
        p.exp_date_eq = Some("2024-01-26".into());
        p.tradetime_from = Some("2024-01-01".into());
        p.tradetime_to = Some("2024-01-02".into());
        p.expiration_type = Some("monthly".into());
        p.sort = Some("-tradetime".into());
        p.page_offset = Some(0);
        p.page_limit = Some(50);
        p.fields = Some("contract,bid,ask,delta".into());
        p.compact = Some(true);

        let q = build_options_query(&p);

        let pairs: std::collections::HashMap<&str, &str> =
            q.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        assert_eq!(pairs["filter[underlying_symbol]"], "AAPL");
        assert_eq!(pairs["filter[contract]"], "AAPL250321C00150000");
        assert_eq!(pairs["filter[type]"], "call");
        assert_eq!(pairs["filter[strike_from]"], "150");
        assert_eq!(pairs["filter[strike_to]"], "200.5");
        assert_eq!(pairs["filter[exp_date_from]"], "2024-01-21");
        assert_eq!(pairs["filter[exp_date_to]"], "2024-01-28");
        assert_eq!(pairs["filter[exp_date_eq]"], "2024-01-26");
        assert_eq!(pairs["filter[tradetime_from]"], "2024-01-01");
        assert_eq!(pairs["filter[tradetime_to]"], "2024-01-02");
        assert_eq!(pairs["filter[expiration_type]"], "monthly");
        assert_eq!(pairs["sort"], "-tradetime");
        assert_eq!(pairs["page[offset]"], "0");
        assert_eq!(pairs["page[limit]"], "50");
        assert_eq!(pairs["fields[options-eod]"], "contract,bid,ask,delta");
        assert_eq!(pairs["compact"], "1");
    }

    #[test]
    fn page_limit_is_clamped_to_1000() {
        let mut p = empty_params("eod");
        p.page_limit = Some(5000);
        let q = build_options_query(&p);
        let lim = q
            .iter()
            .find(|(k, _)| k == "page[limit]")
            .map(|(_, v)| v.as_str())
            .unwrap();
        assert_eq!(lim, "1000");
    }

    #[test]
    fn fields_key_is_mode_specific() {
        let mut p = empty_params("contracts");
        p.fields = Some("strike,exp_date".into());
        let q = build_options_query(&p);
        assert!(q
            .iter()
            .any(|(k, v)| k == "fields[options-contracts]" && v == "strike,exp_date"));
        assert!(!q.iter().any(|(k, _)| k == "fields[options-eod]"));
    }

    #[test]
    fn fields_dropped_for_underlyings_mode() {
        let mut p = empty_params("underlyings");
        p.fields = Some("anything".into());
        assert!(build_options_query(&p).is_empty());
    }

    #[test]
    fn scrub_token_redacts_single_occurrence() {
        let url = "https://eodhd.com/api/x?foo=bar&api_token=SECRET123&page=2";
        assert_eq!(
            scrub_token(url),
            "https://eodhd.com/api/x?foo=bar&api_token=***&page=2"
        );
    }

    #[test]
    fn scrub_token_redacts_at_end_of_string() {
        let url = "https://eodhd.com/api/x?api_token=SECRET";
        assert_eq!(scrub_token(url), "https://eodhd.com/api/x?api_token=***");
    }

    #[test]
    fn scrub_token_redacts_multiple_occurrences() {
        let s = "first api_token=A&middle&api_token=B end";
        assert_eq!(
            scrub_token(s),
            "first api_token=***&middle&api_token=*** end"
        );
    }

    #[test]
    fn scrub_token_no_op_when_missing() {
        let s = "no token here";
        assert_eq!(scrub_token(s), "no token here");
    }

    #[test]
    fn flatten_jsonapi_hoists_attributes() {
        let item = serde_json::json!({
            "id": "AAPL231117C00300000-2023-11-17",
            "type": "options-eod",
            "attributes": {
                "contract": "AAPL231117C00300000",
                "strike": 300,
                "bid": null
            }
        });
        let flat = flatten_jsonapi(&item);
        assert_eq!(flat["contract"], "AAPL231117C00300000");
        assert_eq!(flat["strike"], 300);
        assert!(flat["bid"].is_null());
        assert_eq!(flat["_id"], "AAPL231117C00300000-2023-11-17");
    }

    #[test]
    fn flatten_jsonapi_passthrough_for_flat_object() {
        let item = serde_json::json!({"contract": "X", "strike": 100});
        let flat = flatten_jsonapi(&item);
        assert_eq!(flat, item);
    }

    #[test]
    fn parses_canned_eod_response() {
        let body = r#"{
          "meta": {"offset": 0, "limit": 1000, "total": 2},
          "data": [
            {
              "id": "AAPL231117C00300000-2023-11-17",
              "type": "options-eod",
              "attributes": {
                "contract": "AAPL231117C00300000",
                "underlying_symbol": "AAPL",
                "exp_date": "2023-11-17",
                "type": "call",
                "strike": 300,
                "bid": null,
                "ask": 0.01,
                "open_interest": 2300,
                "delta": 0,
                "tradetime": "2023-11-17"
              }
            }
          ],
          "links": {"next": "https://eodhd.com/api/mp/unicornbay/options/eod?page%5Boffset%5D=1000&api_token=ABC"}
        }"#;
        let v: Value = serde_json::from_str(body).unwrap();
        let row = flatten_jsonapi(&v["data"][0]);
        assert_eq!(row["contract"], "AAPL231117C00300000");
        assert!(row["bid"].is_null());
        assert_eq!(row["strike"], 300);

        let next = v["links"]["next"].as_str().unwrap();
        assert!(scrub_token(next).contains("api_token=***"));
        assert!(!scrub_token(next).contains("ABC"));
    }
}
