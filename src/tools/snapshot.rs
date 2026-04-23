//! `snapshot` tool — single-call financial profile (spec §5.2).
//!
//! Composes the previously-required cascade (General + Highlights +
//! Valuation + SharesStats + Financials::*) into one call, plus derived
//! analytics (margins, leverage, FCF yield, YoY growth).
//!
//! Source data is one call to `/fundamentals/{symbol}` (cached). Spot
//! price / 52-week range are intentionally NOT included — callers can
//! reach for the `price` tool when they need live market context.

use crate::analytics::normalization::{as_f64, field_f64, sorted_dates_desc};
use crate::analytics::ratios::{compute_ratios, RatioSet};
use crate::analytics::ttm::{ttm_quarterly, ttm_quarterly_signed, yoy_growth};
use crate::cache::Cache;
use crate::client::EodhdClient;
use crate::format::{render_envelope, Metadata};
use crate::tools::fetch::{fundamentals_trimmed, most_recent_filing};
use serde::Serialize;
use serde_json::{json, Value};

/// Number of quarterly periods we slice fundamentals to before deriving.
/// 8 = 4 for TTM rollup + 4 prior for YoY comparison. Anything beyond is
/// noise that hurts cache size for no analytical gain.
const SNAPSHOT_PERIODS: usize = 8;

#[derive(Debug, Default, Serialize)]
pub struct CompanyInfo {
    pub name: Option<String>,
    pub sector: Option<String>,
    pub industry: Option<String>,
    pub country: Option<String>,
    pub fiscal_year_end: Option<String>,
    pub employees: Option<i64>,
    pub exchange: Option<String>,
    pub currency: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct MarketBlock {
    pub market_cap: Option<f64>,
    pub enterprise_value: Option<f64>,
    pub target_price: Option<f64>,
    pub pe_trailing: Option<f64>,
    pub pe_forward: Option<f64>,
    pub ps_ratio: Option<f64>,
    pub pb_ratio: Option<f64>,
    pub ev_revenue: Option<f64>,
    pub ev_ebitda: Option<f64>,
}

#[derive(Debug, Default, Serialize)]
pub struct ProfitabilityBlock {
    pub gross_margin: Option<f64>,
    pub operating_margin: Option<f64>,
    pub ebitda_margin: Option<f64>,
    pub net_margin: Option<f64>,
    pub roe: Option<f64>,
    pub roa: Option<f64>,
    pub roic: Option<f64>,
}

#[derive(Debug, Default, Serialize)]
pub struct BalanceBlock {
    pub current_ratio: Option<f64>,
    pub quick_ratio: Option<f64>,
    pub cash_ratio: Option<f64>,
    pub debt_to_equity: Option<f64>,
    pub net_debt: Option<f64>,
    pub net_debt_to_ebitda: Option<f64>,
    pub interest_coverage: Option<f64>,
}

#[derive(Debug, Default, Serialize)]
pub struct CashFlowBlock {
    pub cfo_ttm: Option<f64>,
    pub capex_ttm: Option<f64>,
    pub fcf_ttm: Option<f64>,
    pub fcf_yield: Option<f64>,
    pub capex_intensity: Option<f64>,
}

#[derive(Debug, Default, Serialize)]
pub struct GrowthBlock {
    /// Revenue YoY most-recent quarter.
    pub revenue_yoy_q: Option<f64>,
    /// Net income YoY most-recent quarter.
    pub net_income_yoy_q: Option<f64>,
    /// Diluted EPS YoY (from Highlights when available).
    pub eps_yoy: Option<f64>,
}

#[derive(Debug, Default, Serialize)]
pub struct ShareholderBlock {
    pub dividend_yield: Option<f64>,
    pub dividend_per_share: Option<f64>,
    pub buyback_ttm: Option<f64>,
    pub insider_pct: Option<f64>,
    pub institutional_pct: Option<f64>,
}

#[derive(Debug, Default, Serialize)]
pub struct DataFreshness {
    pub last_reported_quarter: Option<String>,
    pub last_filing_date: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub ticker: String,
    pub company: CompanyInfo,
    pub market: MarketBlock,
    pub profitability: ProfitabilityBlock,
    pub balance: BalanceBlock,
    pub cash_flow: CashFlowBlock,
    pub growth: GrowthBlock,
    pub shareholder: ShareholderBlock,
    pub data_freshness: DataFreshness,
    pub warnings: Vec<String>,
}

/// Build a full snapshot from a fundamentals payload. Pure function — does
/// no I/O — so tests can run it against fixture JSON.
pub fn snapshot_from_fundamentals(symbol: &str, fundamentals: &Value) -> Snapshot {
    let mut warnings: Vec<String> = Vec::new();

    let general = fundamentals.get("General");
    let highlights = fundamentals.get("Highlights");
    let valuation = fundamentals.get("Valuation");
    let shares = fundamentals.get("SharesStats");

    let company = CompanyInfo {
        name: general.and_then(|g| g.get("Name")).and_then(str_clone),
        sector: general.and_then(|g| g.get("Sector")).and_then(str_clone),
        industry: general.and_then(|g| g.get("Industry")).and_then(str_clone),
        country: general.and_then(|g| g.get("CountryName")).and_then(str_clone),
        fiscal_year_end: general
            .and_then(|g| g.get("FiscalYearEnd"))
            .and_then(str_clone),
        employees: general
            .and_then(|g| g.get("FullTimeEmployees"))
            .and_then(|v| v.as_i64()),
        exchange: general.and_then(|g| g.get("Exchange")).and_then(str_clone),
        currency: general
            .and_then(|g| g.get("CurrencyCode"))
            .and_then(str_clone),
    };

    let ratios: RatioSet = compute_ratios(fundamentals);

    let market = MarketBlock {
        market_cap: highlights
            .and_then(|h| h.get("MarketCapitalization"))
            .and_then(as_f64),
        enterprise_value: valuation
            .and_then(|v| v.get("EnterpriseValue"))
            .and_then(as_f64),
        target_price: highlights
            .and_then(|h| h.get("WallStreetTargetPrice"))
            .and_then(as_f64),
        pe_trailing: ratios.pe,
        pe_forward: ratios.forward_pe,
        ps_ratio: ratios.ps,
        pb_ratio: ratios.pb,
        ev_revenue: ratios.ev_revenue,
        ev_ebitda: ratios.ev_ebitda,
    };

    let profitability = ProfitabilityBlock {
        gross_margin: ratios.gross_margin,
        operating_margin: ratios.operating_margin,
        ebitda_margin: ratios.ebitda_margin,
        net_margin: ratios.net_margin,
        roe: ratios.roe,
        roa: ratios.roa,
        roic: ratios.roic,
    };

    let inc_q = fundamentals
        .pointer("/Financials/Income_Statement/quarterly")
        .and_then(|v| v.as_object());
    let cf_q = fundamentals
        .pointer("/Financials/Cash_Flow/quarterly")
        .and_then(|v| v.as_object());
    let bs_q = fundamentals
        .pointer("/Financials/Balance_Sheet/quarterly")
        .and_then(|v| v.as_object());

    let cfo_ttm = cf_q.and_then(|q| ttm_quarterly(q, "totalCashFromOperatingActivities"));
    let capex_ttm = cf_q.and_then(|q| ttm_quarterly_signed(q, "capitalExpenditures"));
    let fcf_ttm = match (cfo_ttm, capex_ttm) {
        (Some(c), Some(x)) => Some(c - x),
        _ => None,
    };
    let revenue_ttm = inc_q.and_then(|q| ttm_quarterly(q, "totalRevenue"));
    let capex_intensity = match (capex_ttm, revenue_ttm) {
        (Some(x), Some(r)) if r > 0.0 => Some(x / r),
        _ => None,
    };

    let cash_flow = CashFlowBlock {
        cfo_ttm,
        capex_ttm,
        fcf_ttm,
        fcf_yield: ratios.fcf_yield,
        capex_intensity,
    };

    // Net debt for balance block (debt − cash from most recent BS quarter).
    let bs_recent = bs_q.and_then(|q| {
        sorted_dates_desc(q)
            .first()
            .map(|d| q[*d].clone())
    });
    let net_debt = bs_recent.as_ref().and_then(|v| {
        let debt = field_f64(v, "shortLongTermDebtTotal").or_else(|| {
            let s = field_f64(v, "shortTermDebt").unwrap_or(0.0);
            let l = field_f64(v, "longTermDebt").unwrap_or(0.0);
            if s + l > 0.0 {
                Some(s + l)
            } else {
                None
            }
        })?;
        let cash = field_f64(v, "cashAndShortTermInvestments")
            .or_else(|| field_f64(v, "cash"))
            .unwrap_or(0.0);
        Some(debt - cash)
    });

    let balance = BalanceBlock {
        current_ratio: ratios.current_ratio,
        quick_ratio: ratios.quick_ratio,
        cash_ratio: ratios.cash_ratio,
        debt_to_equity: ratios.debt_to_equity,
        net_debt,
        net_debt_to_ebitda: ratios.net_debt_to_ebitda,
        interest_coverage: ratios.interest_coverage,
    };

    let growth = GrowthBlock {
        revenue_yoy_q: inc_q.and_then(|q| yoy_growth(q, "totalRevenue")).or_else(|| {
            highlights
                .and_then(|h| h.get("QuarterlyRevenueGrowthYOY"))
                .and_then(as_f64)
        }),
        net_income_yoy_q: inc_q.and_then(|q| yoy_growth(q, "netIncome")),
        eps_yoy: highlights
            .and_then(|h| h.get("QuarterlyEarningsGrowthYOY"))
            .and_then(as_f64),
    };

    // Buyback TTM = absolute value of repurchaseOfStock TTM.
    let buyback_ttm = cf_q.and_then(|q| ttm_quarterly_signed(q, "repurchaseOfStock"));

    let shareholder = ShareholderBlock {
        dividend_yield: highlights
            .and_then(|h| h.get("DividendYield"))
            .and_then(as_f64),
        dividend_per_share: highlights
            .and_then(|h| h.get("DividendShare"))
            .and_then(as_f64),
        buyback_ttm,
        insider_pct: shares
            .and_then(|s| s.get("PercentInsiders"))
            .and_then(as_f64),
        institutional_pct: shares
            .and_then(|s| s.get("PercentInstitutions"))
            .and_then(as_f64),
    };

    let data_freshness = DataFreshness {
        last_reported_quarter: inc_q.and_then(|q| sorted_dates_desc(q).first().map(|s| s.to_string())),
        last_filing_date: most_recent_filing(fundamentals),
    };

    // Warning: no quarterly data at all.
    if inc_q.is_none() {
        warnings.push("No quarterly income statement found — likely an ETF, fund, or index.".into());
    }
    // Warning: highlights missing (rare but possible).
    if highlights.is_none() {
        warnings.push("Highlights block missing from fundamentals payload.".into());
    }
    // Warning: TTM revenue couldn't be derived.
    if revenue_ttm.is_none() && inc_q.is_some() {
        warnings.push(
            "TTM revenue not derivable — fewer than 4 quarters available, or a quarter is null."
                .into(),
        );
    }

    Snapshot {
        ticker: symbol.to_string(),
        company,
        market,
        profitability,
        balance,
        cash_flow,
        growth,
        shareholder,
        data_freshness,
        warnings,
    }
}

fn str_clone(v: &Value) -> Option<String> {
    v.as_str().map(|s| s.to_string())
}

/// I/O wrapper: fetch + cache + derive + render.
pub async fn run(
    client: &EodhdClient,
    cache: &Cache,
    symbol: &str,
    as_of: &str,
) -> Result<String, String> {
    let cache_was_warm = cache.is_enabled() && {
        let key = Cache::key(
            "fundamentals",
            &[
                ("symbol", symbol),
                ("last_n", &SNAPSHOT_PERIODS.to_string()),
            ],
        );
        cache.get(&key).is_some()
    };

    let fundamentals = fundamentals_trimmed(client, cache, symbol, SNAPSHOT_PERIODS).await?;
    let snap = snapshot_from_fundamentals(symbol, &fundamentals);

    let summary = render_summary(&snap);
    let data = render_data_table(&snap);

    let mut metadata = Metadata::new(as_of)
        .with_source("EODHD_fundamentals")
        .with_source("derived")
        .with_cache_hit(cache_was_warm);

    if let (Some(q), Some(fd)) = (
        snap.data_freshness.last_reported_quarter.as_ref(),
        snap.data_freshness.last_filing_date.as_ref(),
    ) {
        metadata = metadata.with_freshness(format!(
            "last_quarter={}, last_filing={}",
            q, fd
        ));
    } else if let Some(q) = snap.data_freshness.last_reported_quarter.as_ref() {
        metadata = metadata.with_freshness(format!("last_quarter={}", q));
    }
    for w in &snap.warnings {
        metadata = metadata.with_warning(w.clone());
    }

    Ok(render_envelope(&summary, &data, &metadata))
}

fn render_summary(s: &Snapshot) -> String {
    let name = s.company.name.as_deref().unwrap_or(&s.ticker);
    let sector = s.company.sector.as_deref().unwrap_or("?");
    let mut bits: Vec<String> = vec![format!("{} ({}, {}).", name, s.ticker, sector)];

    if let Some(rev_g) = s.growth.revenue_yoy_q {
        bits.push(format!("Revenue YoY {:+.1}% in the latest quarter.", rev_g * 100.0));
    }
    if let Some(om) = s.profitability.operating_margin {
        bits.push(format!("Operating margin TTM {:.1}%.", om * 100.0));
    }
    if let Some(nde) = s.balance.net_debt_to_ebitda {
        bits.push(format!("Net debt / EBITDA {:.2}×.", nde));
    }
    if let Some(fy) = s.cash_flow.fcf_yield {
        bits.push(format!("FCF yield {:.2}%.", fy * 100.0));
    }
    if !s.warnings.is_empty() {
        bits.push(format!("{} warning(s); see metadata.", s.warnings.len()));
    }
    bits.join(" ")
}

fn render_data_table(s: &Snapshot) -> String {
    // The Snapshot is nested — render as pretty JSON so the LLM can
    // navigate the structure cleanly. The envelope wraps this with prose.
    let v: Value = serde_json::to_value(s).unwrap_or(json!({}));
    format!(
        "```json\n{}\n```",
        serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aapl_fixture() -> Value {
        let raw = include_str!("../../tests/fixtures/aapl_fundamentals.json");
        serde_json::from_str(raw).unwrap()
    }

    #[test]
    fn snapshot_extracts_company_info() {
        let s = snapshot_from_fundamentals("AAPL.US", &aapl_fixture());
        assert_eq!(s.company.name.as_deref(), Some("Apple Inc"));
        assert_eq!(s.company.sector.as_deref(), Some("Technology"));
        assert_eq!(s.company.country.as_deref(), Some("USA"));
        assert_eq!(s.company.fiscal_year_end.as_deref(), Some("September"));
        assert_eq!(s.company.employees, Some(164000));
        assert_eq!(s.company.currency.as_deref(), Some("USD"));
    }

    #[test]
    fn snapshot_market_block_uses_highlights_and_valuation() {
        let s = snapshot_from_fundamentals("AAPL.US", &aapl_fixture());
        assert_eq!(s.market.market_cap, Some(3_450_000_000_000.0));
        assert_eq!(s.market.enterprise_value, Some(3_504_000_000_000.0));
        assert_eq!(s.market.pe_trailing, Some(32.5));
        assert_eq!(s.market.pe_forward, Some(28.4));
        assert_eq!(s.market.ev_ebitda, Some(25.58));
    }

    #[test]
    fn snapshot_growth_block_computes_yoy_from_quarters() {
        let s = snapshot_from_fundamentals("AAPL.US", &aapl_fixture());
        // Q4 2024 = 124300; Q4 2023 = 119580 → +3.95%
        let yoy = s.growth.revenue_yoy_q.unwrap();
        assert!((yoy - 0.0395).abs() < 0.01, "expected ~3.95%, got {}", yoy);
    }

    #[test]
    fn snapshot_cash_flow_block_pulls_ttm() {
        let s = snapshot_from_fundamentals("AAPL.US", &aapl_fixture());
        // CFO TTM = 29900+26810+28860+22690 = 108260
        assert_eq!(s.cash_flow.cfo_ttm, Some(108_260_000_000.0));
        // Capex TTM (abs) = 2940+2910+2150+2110 = 10110
        assert_eq!(s.cash_flow.capex_ttm, Some(10_110_000_000.0));
        // FCF = 108260 - 10110 = 98150
        assert_eq!(s.cash_flow.fcf_ttm, Some(98_150_000_000.0));
    }

    #[test]
    fn snapshot_balance_block_computes_net_debt() {
        let s = snapshot_from_fundamentals("AAPL.US", &aapl_fixture());
        // 2024-12-31: total debt 107050M, cash+ST inv 53890M → net debt 53160M
        assert_eq!(s.balance.net_debt, Some(53_160_000_000.0));
        // Net debt / EBITDA TTM = 53160 / 140290 ≈ 0.379
        let nde = s.balance.net_debt_to_ebitda.unwrap();
        assert!((nde - 0.3789).abs() < 0.01);
    }

    #[test]
    fn snapshot_data_freshness_picks_latest_quarter() {
        let s = snapshot_from_fundamentals("AAPL.US", &aapl_fixture());
        assert_eq!(
            s.data_freshness.last_reported_quarter.as_deref(),
            Some("2024-12-31")
        );
        assert_eq!(
            s.data_freshness.last_filing_date.as_deref(),
            Some("2025-02-01")
        );
    }

    #[test]
    fn snapshot_emits_no_warnings_for_complete_aapl_fixture() {
        let s = snapshot_from_fundamentals("AAPL.US", &aapl_fixture());
        assert!(s.warnings.is_empty(), "unexpected warnings: {:?}", s.warnings);
    }

    #[test]
    fn snapshot_warns_when_no_quarterly_data() {
        let mut v = aapl_fixture();
        v["Financials"]["Income_Statement"]["quarterly"] = json!({});
        let s = snapshot_from_fundamentals("AAPL.US", &v);
        assert!(s
            .warnings
            .iter()
            .any(|w| w.contains("TTM revenue not derivable")));
    }

    #[test]
    fn render_summary_mentions_revenue_growth_and_margins() {
        let s = snapshot_from_fundamentals("AAPL.US", &aapl_fixture());
        let summary = render_summary(&s);
        assert!(summary.contains("Apple Inc"));
        assert!(summary.contains("Technology"));
        assert!(summary.contains("Revenue YoY"));
        assert!(summary.contains("Operating margin"));
        assert!(summary.contains("FCF yield"));
    }
}
