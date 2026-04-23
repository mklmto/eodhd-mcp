//! `health_check` tool — five-dimension financial health scorecard with
//! red-flag drill-down (spec §5.2 + Appendix C).
//!
//! Five dimensions: Profitability, Liquidity, Solvency, Efficiency, Growth.
//! Each scored 0–100 from a small set of band-based sub-criteria; the
//! composite is the simple average. Red flags are pulled from the
//! `analytics::anomaly` module (Z-score outliers, CFO-vs-NI streak,
//! revenue decline streak, negative-retained-with-buyback) plus
//! threshold checks on the `RatioSet` (high leverage, low coverage).

use crate::analytics::anomaly::{
    cfo_below_net_income_streak, detect_outliers, negative_retained_with_buyback,
    revenue_decline_streak, Outlier,
};
use crate::analytics::normalization::sorted_dates_desc;
use crate::analytics::ratios::{compute_ratios, RatioSet};
use crate::analytics::ttm::yoy_growth;
use crate::cache::Cache;
use crate::client::EodhdClient;
use crate::format::{render_envelope, Metadata};
use crate::tools::fetch::{fundamentals_trimmed, most_recent_filing};
use crate::tools::financials::humanize_money;
use serde::Serialize;
use serde_json::Value;

const FETCH_PERIODS: usize = 8;

#[derive(Debug, Clone, Copy, Serialize)]
pub enum FlagSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize)]
pub struct RedFlag {
    pub rule: String,
    pub severity: FlagSeverity,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DimensionScore {
    pub name: String,
    pub score: u32,
    pub components: Vec<(String, Option<f64>, u32)>,
}

#[derive(Debug, Serialize)]
pub struct HealthReport {
    pub ticker: String,
    pub composite: u32,
    pub dimensions: Vec<DimensionScore>,
    pub red_flags: Vec<RedFlag>,
    pub last_quarter: Option<String>,
    pub last_filing: Option<String>,
}

/// Score a value with a piecewise-linear band table. `bands` is sorted
/// `(threshold, score)` pairs; the *first* threshold the value is below
/// determines the score. The final pair is the catch-all.
fn band_score(v: Option<f64>, bands: &[(f64, u32)], catch_all: u32) -> u32 {
    let x = match v {
        Some(x) if x.is_finite() => x,
        _ => return 0,
    };
    for (threshold, score) in bands {
        if x < *threshold {
            return *score;
        }
    }
    catch_all
}

fn avg_scores(xs: &[u32]) -> u32 {
    if xs.is_empty() {
        return 0;
    }
    let sum: u32 = xs.iter().sum();
    sum / xs.len() as u32
}

fn score_profitability(r: &RatioSet) -> DimensionScore {
    // Higher is better — pick increasing bands.
    let gm = band_score(
        r.gross_margin,
        &[(0.10, 20), (0.20, 40), (0.30, 60), (0.50, 80)],
        100,
    );
    let om = band_score(
        r.operating_margin,
        &[(0.0, 0), (0.05, 40), (0.15, 60), (0.25, 80)],
        100,
    );
    let nm = band_score(
        r.net_margin,
        &[(0.0, 0), (0.05, 40), (0.15, 70)],
        100,
    );
    let roe = band_score(r.roe, &[(0.0, 0), (0.10, 40), (0.20, 70)], 100);
    let roic = band_score(r.roic, &[(0.05, 20), (0.10, 60), (0.20, 80)], 100);

    DimensionScore {
        name: "Profitability".into(),
        score: avg_scores(&[gm, om, nm, roe, roic]),
        components: vec![
            ("gross_margin".into(), r.gross_margin, gm),
            ("operating_margin".into(), r.operating_margin, om),
            ("net_margin".into(), r.net_margin, nm),
            ("roe".into(), r.roe, roe),
            ("roic".into(), r.roic, roic),
        ],
    }
}

fn score_liquidity(r: &RatioSet) -> DimensionScore {
    // Sweet spot for current ratio is 1.5–3; very high suggests idle cash.
    let cur = match r.current_ratio {
        Some(x) if x >= 1.5 && x <= 3.0 => 100,
        Some(x) if x >= 1.0 => 70,
        Some(x) if x >= 3.0 => 80,
        Some(_) => 40,
        None => 0,
    };
    let qr = band_score(r.quick_ratio, &[(0.5, 40), (1.0, 70)], 100);
    let cr = band_score(r.cash_ratio, &[(0.10, 40), (0.30, 70)], 100);

    DimensionScore {
        name: "Liquidity".into(),
        score: avg_scores(&[cur, qr, cr]),
        components: vec![
            ("current_ratio".into(), r.current_ratio, cur),
            ("quick_ratio".into(), r.quick_ratio, qr),
            ("cash_ratio".into(), r.cash_ratio, cr),
        ],
    }
}

fn score_solvency(r: &RatioSet) -> DimensionScore {
    // Lower is better — invert the band logic.
    let nde = match r.net_debt_to_ebitda {
        Some(x) if x < 0.0 => 100, // net cash
        Some(x) if x < 1.0 => 90,
        Some(x) if x < 2.0 => 70,
        Some(x) if x < 3.0 => 50,
        Some(x) if x < 4.0 => 30,
        Some(_) => 10,
        None => 50, // can't assess → neutral
    };
    let ic = band_score(
        r.interest_coverage,
        &[(1.0, 0), (3.0, 40), (5.0, 70)],
        100,
    );
    let de = match r.debt_to_equity {
        Some(x) if x < 0.5 => 100,
        Some(x) if x < 1.0 => 80,
        Some(x) if x < 2.0 => 60,
        Some(_) => 30,
        None => 50,
    };

    DimensionScore {
        name: "Solvency".into(),
        score: avg_scores(&[nde, ic, de]),
        components: vec![
            ("net_debt_to_ebitda".into(), r.net_debt_to_ebitda, nde),
            ("interest_coverage".into(), r.interest_coverage, ic),
            ("debt_to_equity".into(), r.debt_to_equity, de),
        ],
    }
}

fn score_efficiency(r: &RatioSet) -> DimensionScore {
    let at = band_score(r.asset_turnover, &[(0.3, 40), (0.7, 70)], 100);
    let it = band_score(r.inventory_turnover, &[(2.0, 40), (6.0, 70)], 100);
    // DSO: lower is better.
    let dso = match r.dso {
        Some(x) if x < 60.0 => 100,
        Some(x) if x < 120.0 => 60,
        Some(_) => 20,
        None => 50,
    };
    DimensionScore {
        name: "Efficiency".into(),
        score: avg_scores(&[at, it, dso]),
        components: vec![
            ("asset_turnover".into(), r.asset_turnover, at),
            ("inventory_turnover".into(), r.inventory_turnover, it),
            ("dso".into(), r.dso, dso),
        ],
    }
}

fn score_growth(rev_yoy: Option<f64>, ni_yoy: Option<f64>) -> DimensionScore {
    let rev = match rev_yoy {
        Some(x) if x < 0.0 => 20,
        Some(x) if x < 0.05 => 50,
        Some(x) if x < 0.10 => 70,
        Some(x) if x < 0.25 => 85,
        Some(_) => 100,
        None => 50,
    };
    let ni = match ni_yoy {
        Some(x) if x < 0.0 => 20,
        Some(x) if x < 0.05 => 50,
        Some(x) if x < 0.10 => 70,
        Some(x) if x < 0.25 => 85,
        Some(_) => 100,
        None => 50,
    };
    DimensionScore {
        name: "Growth".into(),
        score: avg_scores(&[rev, ni]),
        components: vec![
            ("revenue_yoy".into(), rev_yoy, rev),
            ("net_income_yoy".into(), ni_yoy, ni),
        ],
    }
}

/// Pure derivation — testable without HTTP.
pub fn build_report(symbol: &str, fundamentals: &Value) -> HealthReport {
    let r = compute_ratios(fundamentals);

    let inc_q = fundamentals
        .pointer("/Financials/Income_Statement/quarterly")
        .and_then(|v| v.as_object());
    let bs_q = fundamentals
        .pointer("/Financials/Balance_Sheet/quarterly")
        .and_then(|v| v.as_object());
    let cf_q = fundamentals
        .pointer("/Financials/Cash_Flow/quarterly")
        .and_then(|v| v.as_object());

    let rev_yoy = inc_q.and_then(|q| yoy_growth(q, "totalRevenue"));
    let ni_yoy = inc_q.and_then(|q| yoy_growth(q, "netIncome"));

    let dimensions = vec![
        score_profitability(&r),
        score_liquidity(&r),
        score_solvency(&r),
        score_efficiency(&r),
        score_growth(rev_yoy, ni_yoy),
    ];

    let composite = avg_scores(&dimensions.iter().map(|d| d.score).collect::<Vec<_>>());

    let mut red_flags: Vec<RedFlag> = Vec::new();

    // Rule: net debt / EBITDA > 3
    if let Some(x) = r.net_debt_to_ebitda {
        if x > 3.0 {
            red_flags.push(RedFlag {
                rule: "Net debt / EBITDA > 3×".into(),
                severity: if x > 5.0 {
                    FlagSeverity::Critical
                } else {
                    FlagSeverity::Warning
                },
                evidence: format!("Currently {:.2}×", x),
            });
        }
    }

    // Rule: interest coverage < 3
    if let Some(x) = r.interest_coverage {
        if x < 3.0 {
            red_flags.push(RedFlag {
                rule: "Interest coverage < 3×".into(),
                severity: if x < 1.0 {
                    FlagSeverity::Critical
                } else {
                    FlagSeverity::Warning
                },
                evidence: format!("Currently {:.2}×", x),
            });
        }
    }

    // Rule: revenue decline 3 consecutive quarters (spec Appendix C)
    if let Some(q) = inc_q {
        if revenue_decline_streak(q, 3) {
            red_flags.push(RedFlag {
                rule: "Revenue YoY < 0 for 3 consecutive quarters".into(),
                severity: FlagSeverity::Warning,
                evidence: "see Income Statement quarterly trend".into(),
            });
        }
    }

    // Rule: CFO < net income for 3 consecutive quarters (earnings quality)
    if let (Some(inc), Some(cf)) = (inc_q, cf_q) {
        if cfo_below_net_income_streak(inc, cf, 3) {
            red_flags.push(RedFlag {
                rule: "Operating cash flow below net income for 3 consecutive quarters".into(),
                severity: FlagSeverity::Warning,
                evidence: "Earnings quality concern — accruals diverging from cash".into(),
            });
        }
    }

    // Rule: negative retained earnings + active buyback
    if let (Some(bs), Some(cf)) = (bs_q, cf_q) {
        if let Some((re, bb)) = negative_retained_with_buyback(bs, cf) {
            red_flags.push(RedFlag {
                rule: "Negative retained earnings combined with active buyback".into(),
                severity: FlagSeverity::Info,
                evidence: format!(
                    "Retained earnings {} + TTM buyback {}",
                    humanize_money(re),
                    humanize_money(bb)
                ),
            });
        }
    }

    // Rule: Z-score outliers on otherNonCashItems (proxy for non-recurring charges)
    if let Some(cf) = cf_q {
        let outliers: Vec<Outlier> = detect_outliers(cf, "otherNonCashItems");
        for o in outliers {
            red_flags.push(RedFlag {
                rule: format!("Non-recurring item flagged on {}", o.field),
                severity: match o.severity {
                    crate::analytics::anomaly::Severity::Material => FlagSeverity::Warning,
                    crate::analytics::anomaly::Severity::Watch => FlagSeverity::Info,
                },
                evidence: format!(
                    "{}: {} (z-score {:+.2})",
                    o.period,
                    humanize_money(o.value),
                    o.z_score
                ),
            });
        }
    }

    // Rule: gross margin Z-score outlier (spec Appendix C)
    if let Some(q) = inc_q {
        // Build a synthetic series of gross_margin per period as %.
        let dates = sorted_dates_desc(q);
        let gms: Vec<f64> = dates
            .iter()
            .filter_map(|d| {
                let rev = q[*d]
                    .get("totalRevenue")
                    .and_then(crate::analytics::normalization::as_f64)?;
                let gp = q[*d]
                    .get("grossProfit")
                    .and_then(crate::analytics::normalization::as_f64)?;
                if rev > 0.0 {
                    Some(gp / rev)
                } else {
                    None
                }
            })
            .collect();
        if let Some((mean, std)) =
            crate::analytics::anomaly::mean_std(&gms)
        {
            for (i, d) in dates.iter().enumerate() {
                if i >= gms.len() {
                    break;
                }
                let z = (gms[i] - mean) / std;
                if z.abs() >= crate::analytics::anomaly::Z_WATCH {
                    red_flags.push(RedFlag {
                        rule: "Gross margin deviation > 2σ from sample mean".into(),
                        severity: FlagSeverity::Info,
                        evidence: format!(
                            "{}: gm {:.2}% (z {:+.2})",
                            d,
                            gms[i] * 100.0,
                            z
                        ),
                    });
                    break; // one entry per rule is enough
                }
            }
        }
    }

    HealthReport {
        ticker: symbol.to_string(),
        composite,
        dimensions,
        red_flags,
        last_quarter: inc_q.and_then(|q| sorted_dates_desc(q).first().map(|s| s.to_string())),
        last_filing: most_recent_filing(fundamentals),
    }
}

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
                ("last_n", &FETCH_PERIODS.to_string()),
            ],
        );
        cache.get(&key).is_some()
    };
    let fundamentals = fundamentals_trimmed(client, cache, symbol, FETCH_PERIODS).await?;
    let report = build_report(symbol, &fundamentals);

    let summary = render_summary(&report);
    let data = render_data(&report);

    let mut metadata = Metadata::new(as_of)
        .with_source("EODHD_fundamentals")
        .with_source("derived")
        .with_cache_hit(cache_was_warm);
    if let (Some(q), Some(fd)) = (report.last_quarter.as_ref(), report.last_filing.as_ref()) {
        metadata = metadata.with_freshness(format!("last_quarter={}, last_filing={}", q, fd));
    } else if let Some(q) = report.last_quarter.as_ref() {
        metadata = metadata.with_freshness(format!("last_quarter={}", q));
    }
    if report.red_flags.is_empty() {
        metadata = metadata.with_warning("No red flags triggered.");
    }

    Ok(render_envelope(&summary, &data, &metadata))
}

fn render_summary(r: &HealthReport) -> String {
    let verdict = match r.composite {
        85..=u32::MAX => "Strong",
        70..=84 => "Healthy",
        55..=69 => "Mixed",
        40..=54 => "Weak",
        _ => "Distressed",
    };
    format!(
        "{} ({}). Composite health score {}/100 — {}. {} red flag(s) raised; see Data section.",
        r.ticker, verdict, r.composite, verdict, r.red_flags.len()
    )
}

fn render_data(r: &HealthReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "**Composite score: {}/100**\n\n",
        r.composite
    ));

    out.push_str("### Dimensions\n\n");
    out.push_str("| Dimension | Score | Sub-criteria (value · score) |\n");
    out.push_str("| --- | ---: | --- |\n");
    for d in &r.dimensions {
        let parts: Vec<String> = d
            .components
            .iter()
            .map(|(label, val, score)| {
                let v_str = match val {
                    Some(x) => format!("{:.3}", x),
                    None => "—".to_string(),
                };
                format!("{}={}·{}", label, v_str, score)
            })
            .collect();
        out.push_str(&format!(
            "| {} | {}/100 | {} |\n",
            d.name,
            d.score,
            parts.join("; ")
        ));
    }

    out.push_str("\n### Red flags\n\n");
    if r.red_flags.is_empty() {
        out.push_str("*None triggered.*\n");
    } else {
        out.push_str("| Severity | Rule | Evidence |\n");
        out.push_str("| --- | --- | --- |\n");
        for f in &r.red_flags {
            let sev = match f.severity {
                FlagSeverity::Critical => "CRITICAL",
                FlagSeverity::Warning => "WARNING",
                FlagSeverity::Info => "INFO",
            };
            out.push_str(&format!("| {} | {} | {} |\n", sev, f.rule, f.evidence));
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
    fn report_composite_is_in_range() {
        let r = build_report("AAPL.US", &aapl_fixture());
        assert!(r.composite <= 100);
        // AAPL with high margins, low leverage, decent growth → expect Strong.
        assert!(
            r.composite >= 70,
            "AAPL composite suspiciously low: {}",
            r.composite
        );
    }

    #[test]
    fn report_has_five_dimensions() {
        let r = build_report("AAPL.US", &aapl_fixture());
        assert_eq!(r.dimensions.len(), 5);
        let names: Vec<&str> = r.dimensions.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Profitability",
                "Liquidity",
                "Solvency",
                "Efficiency",
                "Growth"
            ]
        );
    }

    #[test]
    fn report_flags_synthetic_other_non_cash_spike() {
        let r = build_report("AAPL.US", &aapl_fixture());
        assert!(
            r.red_flags
                .iter()
                .any(|f| f.rule.contains("Non-recurring item")),
            "expected the 2024-09-30 spike to surface as a red flag; got {:?}",
            r.red_flags
        );
    }

    #[test]
    fn report_records_last_quarter_and_filing() {
        let r = build_report("AAPL.US", &aapl_fixture());
        assert_eq!(r.last_quarter.as_deref(), Some("2024-12-31"));
        assert_eq!(r.last_filing.as_deref(), Some("2025-02-01"));
    }

    #[test]
    fn band_score_picks_first_matching_band() {
        // value 0.07: under 0.10 → 20; under 0.20 not reached
        assert_eq!(
            band_score(Some(0.07), &[(0.10, 20), (0.20, 40), (0.30, 60)], 100),
            20
        );
        // value 0.25: under 0.30 → 60
        assert_eq!(
            band_score(Some(0.25), &[(0.10, 20), (0.20, 40), (0.30, 60)], 100),
            60
        );
        // value 0.50: catch-all
        assert_eq!(
            band_score(Some(0.50), &[(0.10, 20), (0.20, 40), (0.30, 60)], 100),
            100
        );
        // None → 0
        assert_eq!(band_score(None, &[(0.10, 20)], 100), 0);
    }

    #[test]
    fn render_summary_picks_correct_verdict_label() {
        let mut r = build_report("AAPL.US", &aapl_fixture());
        r.composite = 90;
        assert!(render_summary(&r).contains("Strong"));
        r.composite = 75;
        assert!(render_summary(&r).contains("Healthy"));
        r.composite = 60;
        assert!(render_summary(&r).contains("Mixed"));
        r.composite = 45;
        assert!(render_summary(&r).contains("Weak"));
        r.composite = 30;
        assert!(render_summary(&r).contains("Distressed"));
    }
}
