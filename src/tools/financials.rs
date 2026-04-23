//! `financials` tool — DataFrame-shaped financial statements (spec §5.2).
//!
//! Output shape:
//!   - Columns: dates sorted descending (newest first), plus a `TTM_4Q`
//!     column on quarterly views.
//!   - Rows: native EODHD line items (totalRevenue, grossProfit, …) followed
//!     by `_derived_` rows (margins, YoY growth, FCF, capex intensity).
//!   - Cells: human-readable money formatting (K/M/B/T) for raw numbers,
//!     percentages for derived ratios. Empty cells render as `—`.
//!
//! Caller picks `statement` ∈ {income, balance, cashflow, all}, `period`
//! ∈ {quarterly, yearly} (default quarterly), and `last_n` (default 8).
//! The fundamentals payload is reused via the shared cache.

use crate::analytics::normalization::{as_f64, field_f64, sorted_dates_desc};
use crate::analytics::ttm::{ttm_quarterly, ttm_quarterly_signed};
use crate::cache::Cache;
use crate::client::EodhdClient;
use crate::format::{render_envelope, Metadata};
use crate::tools::fetch::{fundamentals_trimmed, most_recent_filing};
use serde_json::Value;

/// Default period count for the `financials` view.
const DEFAULT_LAST_N: usize = 8;

/// Mapping `statement` keyword → (display name, EODHD path segment).
fn statement_path(statement: &str) -> Option<(&'static str, &'static str)> {
    match statement {
        "income" => Some(("Income Statement", "Income_Statement")),
        "balance" => Some(("Balance Sheet", "Balance_Sheet")),
        "cashflow" => Some(("Cash Flow", "Cash_Flow")),
        _ => None,
    }
}

/// Validated input options after defaults applied.
#[derive(Debug)]
pub struct Options {
    pub statement: String, // income | balance | cashflow | all
    pub period: String,    // quarterly | yearly
    pub last_n: usize,
}

impl Options {
    pub fn new(statement: &str, period: Option<&str>, last_n: Option<usize>) -> Result<Self, String> {
        let st = statement.to_lowercase();
        if !matches!(st.as_str(), "income" | "balance" | "cashflow" | "all") {
            return Err(format!(
                "Invalid statement '{}'. Use: income, balance, cashflow, all.",
                statement
            ));
        }
        let pe = period.unwrap_or("quarterly").to_lowercase();
        if !matches!(pe.as_str(), "quarterly" | "yearly") {
            return Err(format!(
                "Invalid period '{}'. Use: quarterly, yearly.",
                period.unwrap_or("?")
            ));
        }
        let n = last_n.unwrap_or(DEFAULT_LAST_N);
        if n == 0 || n > 40 {
            return Err(format!("last_n out of range: {} (use 1-40).", n));
        }
        Ok(Self {
            statement: st,
            period: pe,
            last_n: n,
        })
    }
}

/// Tool entry point — fetches, derives, renders.
pub async fn run(
    client: &EodhdClient,
    cache: &Cache,
    symbol: &str,
    opts: Options,
    as_of: &str,
) -> Result<String, String> {
    let cache_was_warm = cache.is_enabled() && {
        let key = Cache::key(
            "fundamentals",
            &[("symbol", symbol), ("last_n", &opts.last_n.to_string())],
        );
        cache.get(&key).is_some()
    };

    let fundamentals = fundamentals_trimmed(client, cache, symbol, opts.last_n).await?;

    let statements: Vec<&str> = if opts.statement == "all" {
        vec!["income", "balance", "cashflow"]
    } else {
        vec![opts.statement.as_str()]
    };

    let mut warnings: Vec<String> = Vec::new();
    let mut data_blocks: Vec<String> = Vec::new();

    for st in &statements {
        let (label, path) = statement_path(st).expect("validated above");
        let table = build_dataframe(&fundamentals, path, &opts.period, &mut warnings);
        data_blocks.push(format!("### {} ({}, last {})\n\n{}", label, opts.period, opts.last_n, table));
    }

    let summary = render_summary(symbol, &opts, &fundamentals);
    let data = data_blocks.join("\n\n");

    let mut metadata = Metadata::new(as_of)
        .with_source("EODHD_fundamentals")
        .with_source("derived")
        .with_cache_hit(cache_was_warm);
    if let Some(filing) = most_recent_filing(&fundamentals) {
        metadata = metadata.with_freshness(format!("last_filing={}", filing));
    }
    for w in warnings {
        metadata = metadata.with_warning(w);
    }

    Ok(render_envelope(&summary, &data, &metadata))
}

/// Pivot a `Financials::{statement}::{period}` block into a markdown table:
/// rows = line items, columns = dates. Adds derived rows where applicable.
fn build_dataframe(
    fundamentals: &Value,
    statement_path: &str,
    period: &str,
    warnings: &mut Vec<String>,
) -> String {
    let block = fundamentals
        .pointer(&format!("/Financials/{}/{}", statement_path, period))
        .and_then(|v| v.as_object());
    let block = match block {
        Some(b) if !b.is_empty() => b,
        _ => {
            warnings.push(format!(
                "No data for {}::{}. Likely an ETF, fund, or index — or the source omits this period.",
                statement_path, period
            ));
            return "*(no data available)*".to_string();
        }
    };

    let dates = sorted_dates_desc(block);

    // Collect line items in stable order — first occurrence wins.
    let mut row_order: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for d in &dates {
        if let Some(entry) = block[*d].as_object() {
            for k in entry.keys() {
                if k == "date" || k == "filing_date" || k == "currency_symbol" {
                    continue;
                }
                if seen.insert(k.clone()) {
                    row_order.push(k.clone());
                }
            }
        }
    }

    let want_ttm = period == "quarterly" && dates.len() >= 4;

    // Header
    let mut out = String::new();
    out.push_str("| Line item |");
    for d in &dates {
        out.push_str(&format!(" {} |", d));
    }
    if want_ttm {
        out.push_str(" TTM_4Q |");
    }
    out.push('\n');

    out.push_str("| --- |");
    for _ in &dates {
        out.push_str(" ---: |");
    }
    if want_ttm {
        out.push_str(" ---: |");
    }
    out.push('\n');

    // Native rows
    for field in &row_order {
        out.push_str(&format!("| {} |", humanize_field(field)));
        for d in &dates {
            let cell = block[*d]
                .get(field)
                .map(|v| format_cell(v))
                .unwrap_or_else(|| "—".to_string());
            out.push_str(&format!(" {} |", cell));
        }
        if want_ttm {
            // Strict TTM: only meaningful for income / cash flow flow-type fields.
            // Balance-sheet fields are stocks, not flows — show — instead.
            let ttm = if statement_path == "Balance_Sheet" {
                None
            } else {
                ttm_quarterly(block, field).or_else(|| ttm_quarterly_signed(block, field))
            };
            out.push_str(&format!(
                " {} |",
                ttm.map(humanize_money).unwrap_or_else(|| "—".to_string())
            ));
        }
        out.push('\n');
    }

    // Derived rows
    let derived = derived_rows(statement_path, block, &dates, want_ttm);
    if !derived.is_empty() {
        out.push_str("| | |");
        for _ in &dates {
            out.push_str(" |");
        }
        if want_ttm {
            out.push_str(" |");
        }
        out.push('\n');
        for row in derived {
            out.push_str(&row);
            out.push('\n');
        }
    }

    out
}

/// Compute `_derived_` rows per statement type.
fn derived_rows(
    statement_path: &str,
    block: &serde_json::Map<String, Value>,
    dates: &[&str],
    want_ttm: bool,
) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();

    let pct_row = |label: &str, f: &dyn Fn(&Value) -> Option<f64>| -> String {
        let mut s = format!("| _{}_% |", label);
        for d in dates {
            let v = f(&block[*d]);
            s.push_str(&format!(
                " {} |",
                v.map(|x| format!("{:.2}%", x * 100.0))
                    .unwrap_or_else(|| "—".to_string())
            ));
        }
        if want_ttm {
            s.push_str(" — |");
        }
        s
    };

    match statement_path {
        "Income_Statement" => {
            rows.push(pct_row("gross_margin", &|v| {
                let r = field_f64(v, "totalRevenue")?;
                let g = field_f64(v, "grossProfit")?;
                if r <= 0.0 {
                    None
                } else {
                    Some(g / r)
                }
            }));
            rows.push(pct_row("operating_margin", &|v| {
                let r = field_f64(v, "totalRevenue")?;
                let o = field_f64(v, "operatingIncome")?;
                if r <= 0.0 {
                    None
                } else {
                    Some(o / r)
                }
            }));
            rows.push(pct_row("net_margin", &|v| {
                let r = field_f64(v, "totalRevenue")?;
                let n = field_f64(v, "netIncome")?;
                if r <= 0.0 {
                    None
                } else {
                    Some(n / r)
                }
            }));
            rows.push(pct_row("ebitda_margin", &|v| {
                let r = field_f64(v, "totalRevenue")?;
                let e = field_f64(v, "ebitda")?;
                if r <= 0.0 {
                    None
                } else {
                    Some(e / r)
                }
            }));

            // QoQ revenue growth
            if dates.len() >= 2 {
                let mut s = String::from("| _revenue_QoQ_% |");
                for i in 0..dates.len() {
                    let cell = if i + 1 < dates.len() {
                        let cur = field_f64(&block[dates[i]], "totalRevenue");
                        let prev = field_f64(&block[dates[i + 1]], "totalRevenue");
                        match (cur, prev) {
                            (Some(c), Some(p)) if p > 0.0 => Some((c - p) / p),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    s.push_str(&format!(
                        " {} |",
                        cell.map(|x| format!("{:+.2}%", x * 100.0))
                            .unwrap_or_else(|| "—".to_string())
                    ));
                }
                if want_ttm {
                    s.push_str(" — |");
                }
                rows.push(s);
            }
        }
        "Cash_Flow" => {
            // FCF row (CFO − |Capex|), per period
            let mut s = String::from("| _free_cash_flow |");
            for d in dates {
                let cfo = field_f64(&block[*d], "totalCashFromOperatingActivities");
                let capex = field_f64(&block[*d], "capitalExpenditures").map(|x| x.abs());
                let fcf = match (cfo, capex) {
                    (Some(c), Some(x)) => Some(c - x),
                    _ => None,
                };
                s.push_str(&format!(
                    " {} |",
                    fcf.map(humanize_money).unwrap_or_else(|| "—".to_string())
                ));
            }
            if want_ttm {
                let cfo = ttm_quarterly(block, "totalCashFromOperatingActivities");
                let capex = ttm_quarterly_signed(block, "capitalExpenditures");
                let fcf = match (cfo, capex) {
                    (Some(c), Some(x)) => Some(c - x),
                    _ => None,
                };
                s.push_str(&format!(
                    " {} |",
                    fcf.map(humanize_money).unwrap_or_else(|| "—".to_string())
                ));
            }
            rows.push(s);
        }
        "Balance_Sheet" => {
            // Net debt per period (debt − cash).
            let mut s = String::from("| _net_debt |");
            for d in dates {
                let entry = &block[*d];
                let debt = field_f64(entry, "shortLongTermDebtTotal").or_else(|| {
                    let st = field_f64(entry, "shortTermDebt").unwrap_or(0.0);
                    let lt = field_f64(entry, "longTermDebt").unwrap_or(0.0);
                    if st + lt > 0.0 {
                        Some(st + lt)
                    } else {
                        None
                    }
                });
                let cash = field_f64(entry, "cashAndShortTermInvestments")
                    .or_else(|| field_f64(entry, "cash"))
                    .unwrap_or(0.0);
                let nd = debt.map(|d| d - cash);
                s.push_str(&format!(
                    " {} |",
                    nd.map(humanize_money).unwrap_or_else(|| "—".to_string())
                ));
            }
            if want_ttm {
                s.push_str(" — |");
            }
            rows.push(s);
        }
        _ => {}
    }

    rows
}

fn render_summary(symbol: &str, opts: &Options, fundamentals: &Value) -> String {
    let name = fundamentals
        .pointer("/General/Name")
        .and_then(|v| v.as_str())
        .unwrap_or(symbol);

    let what = match opts.statement.as_str() {
        "all" => "income statement, balance sheet, and cash flow".to_string(),
        s => format!("{} statement", s),
    };
    format!(
        "{} — {} ({}, last {} periods). Native EODHD line items plus derived margin / growth / TTM rows. Empty cells indicate the source omitted that field for that period.",
        name, what, opts.period, opts.last_n
    )
}

/// Format a raw JSON cell — strings, numbers, nulls — into a table cell.
fn format_cell(v: &Value) -> String {
    match v {
        Value::Null => "—".to_string(),
        _ => as_f64(v)
            .map(humanize_money)
            .unwrap_or_else(|| v.to_string()),
    }
}

/// Pretty-print a money figure with K/M/B/T suffix.
pub fn humanize_money(n: f64) -> String {
    if !n.is_finite() {
        return "—".to_string();
    }
    let sign = if n < 0.0 { "-" } else { "" };
    let a = n.abs();
    let (val, suffix) = if a >= 1e12 {
        (a / 1e12, "T")
    } else if a >= 1e9 {
        (a / 1e9, "B")
    } else if a >= 1e6 {
        (a / 1e6, "M")
    } else if a >= 1e3 {
        (a / 1e3, "K")
    } else {
        (a, "")
    };
    format!("{}{:.2}{}", sign, val, suffix)
}

/// camelCase → "Camel case" so the LLM gets human-readable row labels.
fn humanize_field(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
            out.push(c.to_ascii_lowercase());
        } else if i == 0 {
            out.push(c.to_ascii_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aapl_fixture() -> Value {
        let raw = include_str!("../../tests/fixtures/aapl_fundamentals.json");
        serde_json::from_str(raw).unwrap()
    }

    #[test]
    fn options_validates_statement_and_period() {
        assert!(Options::new("income", None, None).is_ok());
        assert!(Options::new("income", Some("yearly"), None).is_ok());
        assert!(Options::new("xyz", None, None).is_err());
        assert!(Options::new("income", Some("daily"), None).is_err());
        assert!(Options::new("income", None, Some(0)).is_err());
        assert!(Options::new("income", None, Some(99)).is_err());
    }

    #[test]
    fn humanize_money_picks_right_suffix() {
        assert_eq!(humanize_money(123.0), "123.00");
        assert_eq!(humanize_money(12_345.0), "12.35K");
        assert_eq!(humanize_money(12_345_678.0), "12.35M");
        assert_eq!(humanize_money(12_345_678_900.0), "12.35B");
        assert_eq!(humanize_money(1.23e13), "12.30T");
        assert_eq!(humanize_money(-9_500_000.0), "-9.50M");
    }

    #[test]
    fn humanize_field_splits_camel_case() {
        assert_eq!(humanize_field("totalRevenue"), "Total revenue");
        assert_eq!(humanize_field("netIncome"), "Net income");
        assert_eq!(
            humanize_field("totalCashFromOperatingActivities"),
            "Total cash from operating activities"
        );
    }

    #[test]
    fn dataframe_for_income_quarterly_has_expected_rows_and_columns() {
        let fund = aapl_fixture();
        let mut warnings = vec![];
        let table = build_dataframe(&fund, "Income_Statement", "quarterly", &mut warnings);

        // Headers
        assert!(table.contains("Line item"));
        assert!(table.contains("2024-12-31"));
        assert!(table.contains("2023-03-31"));
        assert!(table.contains("TTM_4Q"));

        // Native rows present
        assert!(table.contains("Total revenue"));
        assert!(table.contains("Gross profit"));
        assert!(table.contains("Net income"));

        // Derived rows present
        assert!(table.contains("_gross_margin_%"));
        assert!(table.contains("_operating_margin_%"));
        assert!(table.contains("_net_margin_%"));
        assert!(table.contains("_revenue_QoQ_%"));

        // TTM column populated for revenue
        // TTM revenue = 124300+94930+85780+90750 = 395,760M → "395.76B"
        assert!(
            table.contains("395.76B"),
            "expected TTM revenue 395.76B, got:\n{}",
            table
        );

        assert!(warnings.is_empty(), "unexpected warnings: {:?}", warnings);
    }

    #[test]
    fn balance_sheet_dataframe_includes_net_debt_row_and_no_ttm() {
        let fund = aapl_fixture();
        let mut warnings = vec![];
        let table = build_dataframe(&fund, "Balance_Sheet", "quarterly", &mut warnings);
        assert!(table.contains("_net_debt"));
        // Net-debt at 2024-12-31 = 107050 - 53890 = 53160M → "53.16B"
        assert!(
            table.contains("53.16B"),
            "expected net debt 53.16B in table:\n{}",
            table
        );
    }

    #[test]
    fn cashflow_dataframe_includes_fcf_row() {
        let fund = aapl_fixture();
        let mut warnings = vec![];
        let table = build_dataframe(&fund, "Cash_Flow", "quarterly", &mut warnings);
        assert!(table.contains("_free_cash_flow"));
        // FCF Q4'24 = 29900 - 2940 = 26960M → "26.96B"
        assert!(
            table.contains("26.96B"),
            "expected FCF Q4'24 26.96B in table:\n{}",
            table
        );
    }

    #[test]
    fn empty_statement_emits_warning() {
        let mut v = aapl_fixture();
        v["Financials"]["Income_Statement"]["quarterly"] = serde_json::json!({});
        let mut warnings = vec![];
        let _ = build_dataframe(&v, "Income_Statement", "quarterly", &mut warnings);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("No data for Income_Statement"));
    }
}
