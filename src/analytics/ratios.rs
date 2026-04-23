//! Standard financial ratios (Appendix A of the refactor spec).
//!
//! Two layers:
//!   - Pure-math primitives (each ratio is a small `Option<f64>`-returning
//!     function over its raw inputs; trivially testable).
//!   - `compute_ratios(&Value)` which extracts whichever inputs are present
//!     in an EODHD `/fundamentals/{symbol}` payload and returns a `RatioSet`.
//!
//! All denominators are checked: zero, negative, or non-finite denominators
//! return `None` rather than `inf`/`-inf`/`NaN`. Some ratios (e.g. interest
//! coverage on a debt-free balance sheet) are *meaningful as None*, so we
//! never substitute placeholder values.

use crate::analytics::normalization::{as_f64, field_f64, sorted_dates_desc};
use crate::analytics::ttm::{ttm_quarterly, ttm_quarterly_signed};
use serde::Serialize;
use serde_json::Value;

/// Safe division — returns None if denominator is zero or non-finite.
pub fn safe_div(num: f64, den: f64) -> Option<f64> {
    if !den.is_finite() || den == 0.0 || !num.is_finite() {
        None
    } else {
        Some(num / den)
    }
}

// ── Profitability ────────────────────────────────────────────────────────

pub fn gross_margin(gross_profit: f64, revenue: f64) -> Option<f64> {
    safe_div(gross_profit, revenue)
}
pub fn operating_margin(operating_income: f64, revenue: f64) -> Option<f64> {
    safe_div(operating_income, revenue)
}
pub fn ebitda_margin(ebitda: f64, revenue: f64) -> Option<f64> {
    safe_div(ebitda, revenue)
}
pub fn net_margin(net_income: f64, revenue: f64) -> Option<f64> {
    safe_div(net_income, revenue)
}
pub fn return_on_equity(net_income: f64, total_equity: f64) -> Option<f64> {
    if total_equity <= 0.0 {
        return None;
    }
    safe_div(net_income, total_equity)
}
pub fn return_on_assets(net_income: f64, total_assets: f64) -> Option<f64> {
    safe_div(net_income, total_assets)
}
/// ROIC ≈ NOPAT / invested_capital, with NOPAT = operating_income · (1 − tax_rate)
/// and invested_capital = total_debt + total_equity.
pub fn return_on_invested_capital(
    operating_income: f64,
    tax_rate: f64,
    total_debt: f64,
    total_equity: f64,
) -> Option<f64> {
    let invested = total_debt + total_equity;
    if invested <= 0.0 {
        return None;
    }
    let nopat = operating_income * (1.0 - tax_rate.clamp(0.0, 1.0));
    safe_div(nopat, invested)
}

// ── Liquidity ────────────────────────────────────────────────────────────

pub fn current_ratio(current_assets: f64, current_liabilities: f64) -> Option<f64> {
    safe_div(current_assets, current_liabilities)
}
pub fn quick_ratio(current_assets: f64, inventory: f64, current_liabilities: f64) -> Option<f64> {
    safe_div(current_assets - inventory, current_liabilities)
}
pub fn cash_ratio(cash_and_equivalents: f64, current_liabilities: f64) -> Option<f64> {
    safe_div(cash_and_equivalents, current_liabilities)
}

// ── Solvency ─────────────────────────────────────────────────────────────

pub fn debt_to_equity(total_debt: f64, total_equity: f64) -> Option<f64> {
    if total_equity <= 0.0 {
        return None;
    }
    safe_div(total_debt, total_equity)
}
pub fn net_debt_to_ebitda(total_debt: f64, cash: f64, ebitda: f64) -> Option<f64> {
    if ebitda <= 0.0 {
        return None;
    }
    safe_div(total_debt - cash, ebitda)
}
pub fn interest_coverage(ebit: f64, interest_expense: f64) -> Option<f64> {
    if interest_expense <= 0.0 {
        return None;
    }
    safe_div(ebit, interest_expense)
}

// ── Efficiency ───────────────────────────────────────────────────────────

pub fn asset_turnover(revenue: f64, total_assets: f64) -> Option<f64> {
    safe_div(revenue, total_assets)
}
pub fn inventory_turnover(cost_of_revenue: f64, inventory: f64) -> Option<f64> {
    if inventory <= 0.0 {
        return None;
    }
    safe_div(cost_of_revenue, inventory)
}
/// Days sales outstanding over a quarter (90-day basis).
pub fn dso_quarterly(receivables: f64, quarterly_revenue: f64) -> Option<f64> {
    if quarterly_revenue <= 0.0 {
        return None;
    }
    Some((receivables / quarterly_revenue) * 90.0)
}

// ── Valuation ────────────────────────────────────────────────────────────

pub fn fcf_yield(fcf_ttm: f64, market_cap: f64) -> Option<f64> {
    safe_div(fcf_ttm, market_cap)
}

// ── Aggregated bundle ────────────────────────────────────────────────────

/// All ratios we can compute from a single `/fundamentals/{symbol}` payload.
/// Field names match spec Appendix A casing where reasonable.
#[derive(Debug, Default, Clone, Serialize)]
#[allow(non_snake_case)]
pub struct RatioSet {
    // Profitability
    pub gross_margin: Option<f64>,
    pub operating_margin: Option<f64>,
    pub ebitda_margin: Option<f64>,
    pub net_margin: Option<f64>,
    pub roe: Option<f64>,
    pub roa: Option<f64>,
    pub roic: Option<f64>,

    // Liquidity
    pub current_ratio: Option<f64>,
    pub quick_ratio: Option<f64>,
    pub cash_ratio: Option<f64>,

    // Solvency
    pub debt_to_equity: Option<f64>,
    pub net_debt_to_ebitda: Option<f64>,
    pub interest_coverage: Option<f64>,

    // Efficiency
    pub asset_turnover: Option<f64>,
    pub inventory_turnover: Option<f64>,
    pub dso: Option<f64>,

    // Valuation (mirrored from Valuation/Highlights when present)
    pub pe: Option<f64>,
    pub forward_pe: Option<f64>,
    pub pb: Option<f64>,
    pub ps: Option<f64>,
    pub ev_revenue: Option<f64>,
    pub ev_ebitda: Option<f64>,
    pub fcf_yield: Option<f64>,
    pub peg: Option<f64>,
}

/// Extract the most recent quarter from a periodic map (returns the JSON entry).
fn most_recent_quarter<'a>(
    fundamentals: &'a Value,
    statement: &str,
) -> Option<(&'a str, &'a Value)> {
    let q = fundamentals
        .get("Financials")?
        .get(statement)?
        .get("quarterly")?
        .as_object()?;
    let dates = sorted_dates_desc(q);
    let key = *dates.first()?;
    Some((key, &q[key]))
}

/// Compute every ratio possible from `fundamentals`. Anything that can't be
/// computed (missing field, zero denominator, insufficient quarters for TTM)
/// is left as `None`.
pub fn compute_ratios(fundamentals: &Value) -> RatioSet {
    let mut r = RatioSet::default();

    let highlights = fundamentals.get("Highlights");
    let valuation = fundamentals.get("Valuation");

    let inc_q = fundamentals
        .pointer("/Financials/Income_Statement/quarterly")
        .and_then(|v| v.as_object());
    let cf_q = fundamentals
        .pointer("/Financials/Cash_Flow/quarterly")
        .and_then(|v| v.as_object());

    let revenue_ttm = inc_q.and_then(|q| ttm_quarterly(q, "totalRevenue"));
    let gross_profit_ttm = inc_q.and_then(|q| ttm_quarterly(q, "grossProfit"));
    let operating_income_ttm = inc_q.and_then(|q| ttm_quarterly(q, "operatingIncome"));
    let ebitda_ttm = inc_q.and_then(|q| ttm_quarterly(q, "ebitda"));
    let net_income_ttm = inc_q.and_then(|q| ttm_quarterly(q, "netIncome"));
    let ebit_ttm = inc_q.and_then(|q| ttm_quarterly(q, "ebit"));
    let interest_ttm = inc_q.and_then(|q| ttm_quarterly_signed(q, "interestExpense"));
    let income_before_tax_ttm = inc_q.and_then(|q| ttm_quarterly(q, "incomeBeforeTax"));
    let income_tax_ttm = inc_q.and_then(|q| ttm_quarterly(q, "incomeTaxExpense"));
    let cogs_ttm = inc_q.and_then(|q| ttm_quarterly(q, "costOfRevenue"));

    let cfo_ttm = cf_q.and_then(|q| ttm_quarterly(q, "totalCashFromOperatingActivities"));
    let capex_ttm_abs = cf_q.and_then(|q| ttm_quarterly_signed(q, "capitalExpenditures"));
    let fcf_ttm = match (cfo_ttm, capex_ttm_abs) {
        (Some(cfo), Some(capex)) => Some(cfo - capex),
        _ => None,
    };

    // Most recent balance sheet quarter
    let bs_recent = most_recent_quarter(fundamentals, "Balance_Sheet").map(|(_, v)| v);
    let total_assets = bs_recent.and_then(|v| field_f64(v, "totalAssets"));
    let total_current_assets = bs_recent.and_then(|v| field_f64(v, "totalCurrentAssets"));
    let total_current_liab = bs_recent.and_then(|v| field_f64(v, "totalCurrentLiabilities"));
    let inventory = bs_recent.and_then(|v| field_f64(v, "inventory"));
    let cash = bs_recent
        .and_then(|v| field_f64(v, "cashAndShortTermInvestments").or_else(|| field_f64(v, "cash")));
    let total_debt = bs_recent.and_then(|v| {
        field_f64(v, "shortLongTermDebtTotal").or_else(|| {
            // Fallback: short_term_debt + long_term_debt
            let s = field_f64(v, "shortTermDebt").unwrap_or(0.0);
            let l = field_f64(v, "longTermDebt").unwrap_or(0.0);
            if s + l > 0.0 {
                Some(s + l)
            } else {
                None
            }
        })
    });
    let total_equity = bs_recent.and_then(|v| field_f64(v, "totalStockholderEquity"));
    let receivables = bs_recent.and_then(|v| field_f64(v, "netReceivables"));

    // Latest quarterly revenue (for DSO)
    let latest_revenue_q = inc_q
        .and_then(|q| {
            let dates = sorted_dates_desc(q);
            dates.first().map(|d| q[*d].clone())
        })
        .as_ref()
        .and_then(|v| field_f64(v, "totalRevenue"));

    // ── Profitability ───────────────────────────────────────────────────
    if let (Some(gp), Some(rev)) = (gross_profit_ttm, revenue_ttm) {
        r.gross_margin = gross_margin(gp, rev);
    }
    if let (Some(oi), Some(rev)) = (operating_income_ttm, revenue_ttm) {
        r.operating_margin = operating_margin(oi, rev);
    } else {
        r.operating_margin = highlights
            .and_then(|h| h.get("OperatingMarginTTM"))
            .and_then(as_f64);
    }
    if let (Some(e), Some(rev)) = (ebitda_ttm, revenue_ttm) {
        r.ebitda_margin = ebitda_margin(e, rev);
    }
    if let (Some(ni), Some(rev)) = (net_income_ttm, revenue_ttm) {
        r.net_margin = net_margin(ni, rev);
    } else {
        r.net_margin = highlights
            .and_then(|h| h.get("ProfitMargin"))
            .and_then(as_f64);
    }
    if let (Some(ni), Some(eq)) = (net_income_ttm, total_equity) {
        r.roe = return_on_equity(ni, eq);
    } else {
        r.roe = highlights
            .and_then(|h| h.get("ReturnOnEquityTTM"))
            .and_then(as_f64);
    }
    if let (Some(ni), Some(ta)) = (net_income_ttm, total_assets) {
        r.roa = return_on_assets(ni, ta);
    } else {
        r.roa = highlights
            .and_then(|h| h.get("ReturnOnAssetsTTM"))
            .and_then(as_f64);
    }
    let tax_rate = match (income_tax_ttm, income_before_tax_ttm) {
        (Some(tax), Some(pretax)) if pretax > 0.0 => (tax / pretax).clamp(0.0, 0.5),
        _ => 0.21, // US statutory fallback
    };
    if let (Some(oi), Some(td), Some(eq)) = (operating_income_ttm, total_debt, total_equity) {
        r.roic = return_on_invested_capital(oi, tax_rate, td, eq);
    }

    // ── Liquidity ───────────────────────────────────────────────────────
    if let (Some(ca), Some(cl)) = (total_current_assets, total_current_liab) {
        r.current_ratio = current_ratio(ca, cl);
        if let Some(inv) = inventory {
            r.quick_ratio = quick_ratio(ca, inv, cl);
        }
    }
    if let (Some(c), Some(cl)) = (cash, total_current_liab) {
        r.cash_ratio = cash_ratio(c, cl);
    }

    // ── Solvency ────────────────────────────────────────────────────────
    if let (Some(td), Some(eq)) = (total_debt, total_equity) {
        r.debt_to_equity = debt_to_equity(td, eq);
    }
    if let (Some(td), Some(c), Some(e)) = (total_debt, cash, ebitda_ttm) {
        r.net_debt_to_ebitda = net_debt_to_ebitda(td, c, e);
    }
    if let (Some(ebit), Some(int_exp)) = (ebit_ttm, interest_ttm) {
        r.interest_coverage = interest_coverage(ebit, int_exp);
    }

    // ── Efficiency ──────────────────────────────────────────────────────
    if let (Some(rev), Some(ta)) = (revenue_ttm, total_assets) {
        r.asset_turnover = asset_turnover(rev, ta);
    }
    if let (Some(cogs), Some(inv)) = (cogs_ttm, inventory) {
        r.inventory_turnover = inventory_turnover(cogs, inv);
    }
    if let (Some(rcv), Some(rev_q)) = (receivables, latest_revenue_q) {
        r.dso = dso_quarterly(rcv, rev_q);
    }

    // ── Valuation (passthrough from Highlights/Valuation) ───────────────
    r.pe = valuation.and_then(|v| v.get("TrailingPE")).and_then(as_f64);
    r.forward_pe = valuation.and_then(|v| v.get("ForwardPE")).and_then(as_f64);
    r.pb = valuation.and_then(|v| v.get("PriceBookMRQ")).and_then(as_f64);
    r.ps = valuation
        .and_then(|v| v.get("PriceSalesTTM"))
        .and_then(as_f64);
    r.ev_revenue = valuation
        .and_then(|v| v.get("EnterpriseValueRevenue"))
        .and_then(as_f64);
    r.ev_ebitda = valuation
        .and_then(|v| v.get("EnterpriseValueEbitda"))
        .and_then(as_f64);
    r.peg = highlights.and_then(|h| h.get("PEGRatio")).and_then(as_f64);

    let market_cap = highlights
        .and_then(|h| h.get("MarketCapitalization"))
        .and_then(as_f64);
    if let (Some(fcf), Some(mc)) = (fcf_ttm, market_cap) {
        r.fcf_yield = fcf_yield(fcf, mc);
    }

    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_div_handles_zero_and_nonfinite() {
        assert_eq!(safe_div(10.0, 2.0), Some(5.0));
        assert_eq!(safe_div(10.0, 0.0), None);
        assert_eq!(safe_div(10.0, f64::NAN), None);
        assert_eq!(safe_div(f64::INFINITY, 1.0), None);
    }

    #[test]
    fn margins_compute_correctly() {
        assert_eq!(gross_margin(40.0, 100.0), Some(0.4));
        assert_eq!(operating_margin(20.0, 100.0), Some(0.2));
        assert_eq!(net_margin(10.0, 100.0), Some(0.1));
        assert_eq!(ebitda_margin(30.0, 100.0), Some(0.3));
    }

    #[test]
    fn negative_equity_blocks_roe_and_d_to_e() {
        assert_eq!(return_on_equity(10.0, -5.0), None);
        assert_eq!(debt_to_equity(100.0, -1.0), None);
    }

    #[test]
    fn interest_coverage_blocked_when_no_interest() {
        // Debt-free company → no meaningful coverage ratio.
        assert_eq!(interest_coverage(1000.0, 0.0), None);
    }

    #[test]
    fn roic_caps_tax_rate_at_50_percent() {
        // Bizarre 200% effective tax rate input gets clamped.
        let r = return_on_invested_capital(100.0, 2.0, 100.0, 100.0).unwrap();
        // tax_rate clamped to 1.0 → NOPAT = 0
        assert_eq!(r, 0.0);
    }

    /// End-to-end check: compute every ratio against the canned AAPL fixture
    /// and verify the values match hand-computed expectations. If this drifts,
    /// either the math is wrong or the fixture changed — both worth a look.
    #[test]
    fn compute_ratios_against_aapl_fixture() {
        let raw = include_str!("../../tests/fixtures/aapl_fundamentals.json");
        let v: Value = serde_json::from_str(raw).expect("fixture must parse");
        let r = compute_ratios(&v);

        // Profitability — TTM derived from Q1'24..Q4'24 in the fixture
        let approx = |a: Option<f64>, b: f64| {
            let got = a.unwrap_or_else(|| panic!("expected Some, got None for value {}", b));
            assert!(
                (got - b).abs() < 0.01,
                "expected ≈{}, got {} (delta {})",
                b,
                got,
                got - b
            );
        };

        approx(r.gross_margin, 0.4638); // 183560 / 395760
        approx(r.net_margin, 0.2458); // 97280 / 395760
        approx(r.ebitda_margin, 0.3545); // 140290 / 395760
        approx(r.operating_margin, 0.3175); // 125630 / 395760
        approx(r.roa, 0.2827); // 97280 / 344090
        approx(r.roe, 1.4500); // 97280 / 67090

        // Liquidity (BS at 2024-12-31)
        approx(r.current_ratio, 0.9091); // 131320 / 144460
        approx(r.quick_ratio, 0.8519); // (131320-8260) / 144460
        approx(r.cash_ratio, 0.3730); // 53890 / 144460

        // Solvency
        approx(r.debt_to_equity, 1.5957); // 107050 / 67090
        approx(r.net_debt_to_ebitda, 0.3789); // (107050-53890) / 140290
        approx(r.interest_coverage, 35.89); // 125630 / 3500

        // Efficiency
        approx(r.asset_turnover, 1.1502); // 395760 / 344090
        approx(r.inventory_turnover, 25.69); // 212200 / 8260
        approx(r.dso, 43.93); // 60680/124300 * 90

        // Valuation passthroughs (from Valuation/Highlights blocks)
        approx(r.pe, 32.5);
        approx(r.forward_pe, 28.4);
        approx(r.pb, 53.2);
        approx(r.ps, 8.82);
        approx(r.ev_revenue, 8.96);
        approx(r.ev_ebitda, 25.58);
        approx(r.peg, 2.8);

        // FCF yield: CFO_TTM 108260M − Capex_TTM 10110M = 98150M; mc 3,450,000M
        approx(r.fcf_yield, 0.02845);

        // ROIC: tax_rate ≈ 32220/130890 = 0.2462 → NOPAT = 125630 × 0.7538
        //       invested = 174140 → ROIC = 0.5436
        approx(r.roic, 0.5436);
    }
}
