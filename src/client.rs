use reqwest::Client;
use serde_json::Value;

const BASE_URL: &str = "https://eodhd.com/api";

#[derive(Clone)]
pub struct EodhdClient {
    http: Client,
    api_key: String,
}

impl EodhdClient {
    pub fn new(api_key: String) -> Self {
        Self {
            http: Client::new(),
            api_key,
        }
    }

    async fn get(&self, path: &str, params: &[(&str, &str)]) -> Result<Value, String> {
        let url = format!("{}{}", BASE_URL, path);
        let mut all_params: Vec<(&str, &str)> = vec![
            ("api_token", &self.api_key),
            ("fmt", "json"),
        ];
        all_params.extend_from_slice(params);

        let resp = self
            .http
            .get(&url)
            .query(&all_params)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        if !status.is_success() {
            return Err(format!("API error (HTTP {}): {}", status, body));
        }

        serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse JSON: {} — body: {}", e, &body[..body.len().min(500)]))
    }

    // ── Search ──────────────────────────────────────────────────────────

    pub async fn search(
        &self,
        query: &str,
        exchange: Option<&str>,
        asset_type: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Value, String> {
        let limit_str = limit.unwrap_or(15).to_string();
        let mut params: Vec<(&str, &str)> = vec![("limit", &limit_str)];
        if let Some(ex) = exchange {
            params.push(("exchange", ex));
        }
        if let Some(t) = asset_type {
            params.push(("type", t));
        }
        self.get(&format!("/search/{}", query), &params).await
    }

    // ── End-of-Day Prices ───────────────────────────────────────────────

    pub async fn eod(
        &self,
        symbol: &str,
        from: Option<&str>,
        to: Option<&str>,
        period: Option<&str>,
        order: Option<&str>,
    ) -> Result<Value, String> {
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = from {
            params.push(("from", v));
        }
        if let Some(v) = to {
            params.push(("to", v));
        }
        if let Some(v) = period {
            params.push(("period", v));
        }
        if let Some(v) = order {
            params.push(("order", v));
        }
        self.get(&format!("/eod/{}", symbol), &params).await
    }

    // ── Intraday Prices ─────────────────────────────────────────────────

    pub async fn intraday(
        &self,
        symbol: &str,
        interval: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<Value, String> {
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = interval {
            params.push(("interval", v));
        }
        if let Some(v) = from {
            params.push(("from", v));
        }
        if let Some(v) = to {
            params.push(("to", v));
        }
        self.get(&format!("/intraday/{}", symbol), &params).await
    }

    // ── Real-Time / Delayed Prices ──────────────────────────────────────

    pub async fn realtime(&self, symbol: &str, extra: Option<&str>) -> Result<Value, String> {
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = extra {
            params.push(("s", v));
        }
        self.get(&format!("/real-time/{}", symbol), &params).await
    }

    // ── Fundamentals ────────────────────────────────────────────────────

    pub async fn fundamentals(
        &self,
        symbol: &str,
        filter: Option<&str>,
    ) -> Result<Value, String> {
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = filter {
            params.push(("filter", v));
        }
        self.get(&format!("/fundamentals/{}", symbol), &params).await
    }

    // ── Dividends ───────────────────────────────────────────────────────

    pub async fn dividends(
        &self,
        symbol: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<Value, String> {
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = from {
            params.push(("from", v));
        }
        if let Some(v) = to {
            params.push(("to", v));
        }
        self.get(&format!("/div/{}", symbol), &params).await
    }

    // ── Splits ──────────────────────────────────────────────────────────

    pub async fn splits(
        &self,
        symbol: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<Value, String> {
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = from {
            params.push(("from", v));
        }
        if let Some(v) = to {
            params.push(("to", v));
        }
        self.get(&format!("/splits/{}", symbol), &params).await
    }

    // ── News ────────────────────────────────────────────────────────────

    pub async fn news(
        &self,
        symbol: Option<&str>,
        topic: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Value, String> {
        let limit_str = limit.unwrap_or(50).to_string();
        let offset_str = offset.unwrap_or(0).to_string();
        let mut params: Vec<(&str, &str)> = vec![
            ("limit", &limit_str),
            ("offset", &offset_str),
        ];
        if let Some(v) = symbol {
            params.push(("s", v));
        }
        if let Some(v) = topic {
            params.push(("t", v));
        }
        if let Some(v) = from {
            params.push(("from", v));
        }
        if let Some(v) = to {
            params.push(("to", v));
        }
        self.get("/news", &params).await
    }

    // ── Sentiment ───────────────────────────────────────────────────────

    pub async fn sentiment(
        &self,
        symbols: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<Value, String> {
        let mut params: Vec<(&str, &str)> = vec![("s", symbols)];
        if let Some(v) = from {
            params.push(("from", v));
        }
        if let Some(v) = to {
            params.push(("to", v));
        }
        self.get("/sentiments", &params).await
    }

    // ── Technical Indicators ────────────────────────────────────────────

    pub async fn technicals(
        &self,
        symbol: &str,
        function: &str,
        period: Option<u32>,
        from: Option<&str>,
        to: Option<&str>,
        order: Option<&str>,
    ) -> Result<Value, String> {
        let period_str = period.map(|p| p.to_string());
        let mut params: Vec<(&str, &str)> = vec![("function", function)];
        if let Some(ref v) = period_str {
            params.push(("period", v));
        }
        if let Some(v) = from {
            params.push(("from", v));
        }
        if let Some(v) = to {
            params.push(("to", v));
        }
        if let Some(v) = order {
            params.push(("order", v));
        }
        self.get(&format!("/technical/{}", symbol), &params).await
    }

    // ── Screener ────────────────────────────────────────────────────────

    pub async fn screener(
        &self,
        filters: Option<&str>,
        signals: Option<&str>,
        sort: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Value, String> {
        let limit_str = limit.unwrap_or(50).to_string();
        let offset_str = offset.unwrap_or(0).to_string();
        let mut params: Vec<(&str, &str)> = vec![
            ("limit", &limit_str),
            ("offset", &offset_str),
        ];
        if let Some(v) = filters {
            params.push(("filters", v));
        }
        if let Some(v) = signals {
            params.push(("signals", v));
        }
        if let Some(v) = sort {
            params.push(("sort", v));
        }
        self.get("/screener", &params).await
    }

    // ── Macro Indicators ────────────────────────────────────────────────

    pub async fn macro_indicator(
        &self,
        country: &str,
        indicator: Option<&str>,
    ) -> Result<Value, String> {
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = indicator {
            params.push(("indicator", v));
        }
        self.get(&format!("/macro-indicator/{}", country), &params)
            .await
    }

    // ── Economic Events ─────────────────────────────────────────────────

    pub async fn economic_events(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        country: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Value, String> {
        let limit_str = limit.unwrap_or(50).to_string();
        let offset_str = offset.unwrap_or(0).to_string();
        let mut params: Vec<(&str, &str)> = vec![
            ("limit", &limit_str),
            ("offset", &offset_str),
        ];
        if let Some(v) = from {
            params.push(("from", v));
        }
        if let Some(v) = to {
            params.push(("to", v));
        }
        if let Some(v) = country {
            params.push(("country", v));
        }
        self.get("/economic-events", &params).await
    }

    // ── Insider Transactions ────────────────────────────────────────────

    pub async fn insider_transactions(
        &self,
        symbol: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Value, String> {
        let limit_str = limit.unwrap_or(100).to_string();
        let mut params: Vec<(&str, &str)> = vec![("limit", &limit_str)];
        if let Some(v) = symbol {
            params.push(("code", v));
        }
        if let Some(v) = from {
            params.push(("from", v));
        }
        if let Some(v) = to {
            params.push(("to", v));
        }
        self.get("/insider-transactions", &params).await
    }

    // ── Calendar (Earnings, IPOs, Splits, Dividends, Trends) ────────────

    pub async fn calendar(
        &self,
        calendar_type: &str,
        symbols: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<Value, String> {
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = symbols {
            params.push(("symbols", v));
        }
        if let Some(v) = from {
            params.push(("from", v));
        }
        if let Some(v) = to {
            params.push(("to", v));
        }
        self.get(&format!("/calendar/{}", calendar_type), &params)
            .await
    }

    // ── Exchange List ───────────────────────────────────────────────────

    pub async fn exchanges_list(&self) -> Result<Value, String> {
        self.get("/exchanges-list/", &[]).await
    }

    // ── Exchange Symbol List ────────────────────────────────────────────

    pub async fn exchange_symbols(
        &self,
        exchange: &str,
        asset_type: Option<&str>,
        delisted: bool,
    ) -> Result<Value, String> {
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = asset_type {
            params.push(("type", v));
        }
        if delisted {
            params.push(("delisted", "1"));
        }
        self.get(&format!("/exchange-symbol-list/{}", exchange), &params)
            .await
    }

    // ── Exchange Details ────────────────────────────────────────────────

    pub async fn exchange_details(
        &self,
        exchange: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<Value, String> {
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = from {
            params.push(("from", v));
        }
        if let Some(v) = to {
            params.push(("to", v));
        }
        self.get(&format!("/exchange-details/{}", exchange), &params)
            .await
    }

    // ── Bulk EOD Data ───────────────────────────────────────────────────

    pub async fn bulk_eod(
        &self,
        exchange: &str,
        data_type: Option<&str>,
        date: Option<&str>,
        symbols: Option<&str>,
    ) -> Result<Value, String> {
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = data_type {
            params.push(("type", v));
        }
        if let Some(v) = date {
            params.push(("date", v));
        }
        if let Some(v) = symbols {
            params.push(("symbols", v));
        }
        self.get(&format!("/eod-bulk-last-day/{}", exchange), &params)
            .await
    }

    // ── US Treasury Rates ───────────────────────────────────────────────

    pub async fn treasury_rates(
        &self,
        rate_type: &str,
        year: Option<u32>,
    ) -> Result<Value, String> {
        let year_str = year.map(|y| y.to_string());
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(ref v) = year_str {
            params.push(("filter[year]", v));
        }
        let path = match rate_type {
            "bill" => "/ust/bill-rates",
            "long_term" => "/ust/long-term-rates",
            "yield" => "/ust/yield-rates",
            "real_yield" => "/ust/real-yield-rates",
            _ => return Err(format!("Invalid treasury rate type: '{}'. Use: bill, long_term, yield, real_yield", rate_type)),
        };
        self.get(path, &params).await
    }

    // ── Historical Market Cap ───────────────────────────────────────────

    pub async fn historical_market_cap(
        &self,
        symbol: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<Value, String> {
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = from {
            params.push(("from", v));
        }
        if let Some(v) = to {
            params.push(("to", v));
        }
        self.get(&format!("/historical-market-cap/{}", symbol), &params)
            .await
    }

    // ── User Account ────────────────────────────────────────────────────

    pub async fn user(&self) -> Result<Value, String> {
        self.get("/user", &[]).await
    }
}
