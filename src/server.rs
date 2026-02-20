use rmcp::{
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::*,
    tool, tool_handler, tool_router,
    ErrorData as McpError, ServerHandler,
};

use crate::client::EodhdClient;
use crate::format::format_value;
use crate::types::*;

fn err(msg: String) -> McpError {
    McpError::internal_error(msg, None)
}

fn ok_text(text: String) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

#[derive(Clone)]
pub struct EodhdServer {
    client: EodhdClient,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl EodhdServer {
    pub fn new(api_key: String) -> Self {
        let client = EodhdClient::new(api_key);
        Self {
            client,
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
            .map_err(|e| err(e))?;
        ok_text(format_value(&format!("Search results for '{}'", params.query), &data))
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
            "eod" => self
                .client
                .eod(
                    &params.symbol,
                    params.from.as_deref(),
                    params.to.as_deref(),
                    params.period.as_deref(),
                    params.order.as_deref(),
                )
                .await,
            "intraday" => self
                .client
                .intraday(
                    &params.symbol,
                    params.interval.as_deref(),
                    params.from.as_deref(),
                    params.to.as_deref(),
                )
                .await,
            "realtime" => self
                .client
                .realtime(&params.symbol, params.extra_symbols.as_deref())
                .await,
            other => Err(format!(
                "Invalid mode '{}'. Use: eod, intraday, realtime",
                other
            )),
        }
        .map_err(|e| err(e))?;

        let label = format!("{} price — {} ({})", params.symbol, params.mode,
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
        description = "Get company fundamental data: general info, financials (income statement, balance sheet, cash flow), highlights, valuation, earnings, ESG scores, analyst ratings, and more. Use the 'filter' parameter to extract specific sections (e.g. 'General', 'Highlights', 'Financials::Balance_Sheet::yearly', 'Valuation'). Without filter returns full data (can be very large). Works for stocks, ETFs, mutual funds, and indices."
    )]
    async fn fundamentals(
        &self,
        Parameters(params): Parameters<FundamentalsParams>,
    ) -> Result<CallToolResult, McpError> {
        let data = self
            .client
            .fundamentals(&params.symbol, params.filter.as_deref())
            .await
            .map_err(|e| err(e))?;
        let label = match &params.filter {
            Some(f) => format!("{} fundamentals ({})", params.symbol, f),
            None => format!("{} fundamentals (full)", params.symbol),
        };
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
            "dividends" => self
                .client
                .dividends(&params.symbol, params.from.as_deref(), params.to.as_deref())
                .await,
            "splits" => self
                .client
                .splits(&params.symbol, params.from.as_deref(), params.to.as_deref())
                .await,
            other => Err(format!(
                "Invalid data_type '{}'. Use: dividends, splits",
                other
            )),
        }
        .map_err(|e| err(e))?;

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
            return Err(err(
                "Either 'symbol' or 'topic' must be provided.".into(),
            ));
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
            .map_err(|e| err(e))?;

        let mut out = format_value("Financial News", &news_data);

        if params.include_sentiment.unwrap_or(false) {
            if let Some(ref sym) = params.symbol {
                match self.client.sentiment(sym, params.from.as_deref(), params.to.as_deref()).await {
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
            .map_err(|e| err(e))?;

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
            .map_err(|e| err(e))?;

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
                let country = params
                    .country
                    .as_deref()
                    .ok_or_else(|| err("'country' is required for indicators mode (ISO Alpha-3, e.g. 'USA')".into()))?;
                self.client
                    .macro_indicator(country, params.indicator.as_deref())
                    .await
            }
            "events" => self
                .client
                .economic_events(
                    params.from.as_deref(),
                    params.to.as_deref(),
                    params.country.as_deref(),
                    params.limit,
                    params.offset,
                )
                .await,
            other => Err(format!(
                "Invalid mode '{}'. Use: indicators, events",
                other
            )),
        }
        .map_err(|e| err(e))?;

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
            .map_err(|e| err(e))?;

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
            .map_err(|e| err(e))?;

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
        .map_err(|e| err(e))?;

        let label = match params.mode.as_str() {
            "list" => "Available Exchanges".to_string(),
            "symbols" => format!(
                "Symbols on {}",
                params.exchange.as_deref().unwrap_or("?")
            ),
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
            .map_err(|e| err(e))?;

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
            .map_err(|e| err(e))?;

        ok_text(format_value(
            &format!(
                "US Treasury — {}{}",
                params.rate_type,
                params
                    .year
                    .map(|y| format!(" ({})", y))
                    .unwrap_or_default()
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
            .historical_market_cap(
                &params.symbol,
                params.from.as_deref(),
                params.to.as_deref(),
            )
            .await
            .map_err(|e| err(e))?;

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
        let data = self.client.user().await.map_err(|e| err(e))?;
        ok_text(format_value("EODHD Account", &data))
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
