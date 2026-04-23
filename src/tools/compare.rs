//! `compare` tool — side-by-side metric comparison across up to 5 tickers
//! (spec §5.2). Fetches fundamentals for every symbol in parallel (each
//! benefits from the shared 7-day fundamentals cache), computes the
//! requested metrics, and emits a comparison table plus a per-metric
//! ranking table where best=1.
//!
//! Recognised metric keys are listed in `METRIC_REGISTRY` below.

use crate::analytics::normalization::as_f64;
use crate::analytics::ratios::{compute_ratios, RatioSet};
use crate::analytics::ttm::yoy_growth;
use crate::cache::Cache;
use crate::client::EodhdClient;
use crate::format::{render_envelope, Metadata};
use crate::tools::fetch::fundamentals_trimmed;
use serde_json::Value;

const MAX_SYMBOLS: usize = 5;
const FETCH_PERIODS: usize = 8;

#[derive(Debug, Clone, Copy)]
enum Direction {
    /// Higher value ranks better (margins, growth, ROE, ...).
    Higher,
    /// Lower value ranks better (debt ratios, valuation multiples, DSO).
    Lower,
}

#[derive(Debug, Clone, Copy)]
enum Format {
    /// 0.247 → "24.70%"
    Percent,
    /// 1.234 → "1.23"
    Ratio,
}

struct MetricSpec {
    key: &'static str,
    label: &'static str,
    direction: Direction,
    format: Format,
}

/// All metrics the tool understands. Extend by adding rows here and an
/// arm in `extract_metric`.
const METRIC_REGISTRY: &[MetricSpec] = &[
    // Profitability — higher is better
    MetricSpec { key: "gross_margin", label: "Gross margin", direction: Direction::Higher, format: Format::Percent },
    MetricSpec { key: "operating_margin", label: "Operating margin", direction: Direction::Higher, format: Format::Percent },
    MetricSpec { key: "ebitda_margin", label: "EBITDA margin", direction: Direction::Higher, format: Format::Percent },
    MetricSpec { key: "net_margin", label: "Net margin", direction: Direction::Higher, format: Format::Percent },
    MetricSpec { key: "roe", label: "ROE", direction: Direction::Higher, format: Format::Percent },
    MetricSpec { key: "roa", label: "ROA", direction: Direction::Higher, format: Format::Percent },
    MetricSpec { key: "roic", label: "ROIC", direction: Direction::Higher, format: Format::Percent },
    // Liquidity — higher is better up to a point; we treat higher as better
    MetricSpec { key: "current_ratio", label: "Current ratio", direction: Direction::Higher, format: Format::Ratio },
    MetricSpec { key: "quick_ratio", label: "Quick ratio", direction: Direction::Higher, format: Format::Ratio },
    MetricSpec { key: "cash_ratio", label: "Cash ratio", direction: Direction::Higher, format: Format::Ratio },
    // Solvency — lower is better (less leverage)
    MetricSpec { key: "debt_to_equity", label: "Debt / equity", direction: Direction::Lower, format: Format::Ratio },
    MetricSpec { key: "net_debt_to_ebitda", label: "Net debt / EBITDA", direction: Direction::Lower, format: Format::Ratio },
    MetricSpec { key: "interest_coverage", label: "Interest coverage", direction: Direction::Higher, format: Format::Ratio },
    // Efficiency
    MetricSpec { key: "asset_turnover", label: "Asset turnover", direction: Direction::Higher, format: Format::Ratio },
    MetricSpec { key: "inventory_turnover", label: "Inventory turnover", direction: Direction::Higher, format: Format::Ratio },
    MetricSpec { key: "dso", label: "DSO (days)", direction: Direction::Lower, format: Format::Ratio },
    // Valuation — lower is better (cheaper)
    MetricSpec { key: "pe", label: "P/E", direction: Direction::Lower, format: Format::Ratio },
    MetricSpec { key: "forward_pe", label: "Forward P/E", direction: Direction::Lower, format: Format::Ratio },
    MetricSpec { key: "ps", label: "P/S", direction: Direction::Lower, format: Format::Ratio },
    MetricSpec { key: "pb", label: "P/B", direction: Direction::Lower, format: Format::Ratio },
    MetricSpec { key: "ev_revenue", label: "EV / revenue", direction: Direction::Lower, format: Format::Ratio },
    MetricSpec { key: "ev_ebitda", label: "EV / EBITDA", direction: Direction::Lower, format: Format::Ratio },
    MetricSpec { key: "fcf_yield", label: "FCF yield", direction: Direction::Higher, format: Format::Percent },
    MetricSpec { key: "peg", label: "PEG", direction: Direction::Lower, format: Format::Ratio },
    // Growth
    MetricSpec { key: "revenue_yoy", label: "Revenue YoY", direction: Direction::Higher, format: Format::Percent },
    MetricSpec { key: "net_income_yoy", label: "Net income YoY", direction: Direction::Higher, format: Format::Percent },
];

fn lookup_spec(key: &str) -> Option<&'static MetricSpec> {
    METRIC_REGISTRY.iter().find(|m| m.key == key)
}

fn extract_metric(key: &str, ratios: &RatioSet, fundamentals: &Value) -> Option<f64> {
    match key {
        "gross_margin" => ratios.gross_margin,
        "operating_margin" => ratios.operating_margin,
        "ebitda_margin" => ratios.ebitda_margin,
        "net_margin" => ratios.net_margin,
        "roe" => ratios.roe,
        "roa" => ratios.roa,
        "roic" => ratios.roic,
        "current_ratio" => ratios.current_ratio,
        "quick_ratio" => ratios.quick_ratio,
        "cash_ratio" => ratios.cash_ratio,
        "debt_to_equity" => ratios.debt_to_equity,
        "net_debt_to_ebitda" => ratios.net_debt_to_ebitda,
        "interest_coverage" => ratios.interest_coverage,
        "asset_turnover" => ratios.asset_turnover,
        "inventory_turnover" => ratios.inventory_turnover,
        "dso" => ratios.dso,
        "pe" => ratios.pe,
        "forward_pe" => ratios.forward_pe,
        "ps" => ratios.ps,
        "pb" => ratios.pb,
        "ev_revenue" => ratios.ev_revenue,
        "ev_ebitda" => ratios.ev_ebitda,
        "fcf_yield" => ratios.fcf_yield,
        "peg" => ratios.peg,
        "revenue_yoy" => fundamentals
            .pointer("/Financials/Income_Statement/quarterly")
            .and_then(|v| v.as_object())
            .and_then(|q| yoy_growth(q, "totalRevenue"))
            .or_else(|| {
                fundamentals
                    .pointer("/Highlights/QuarterlyRevenueGrowthYOY")
                    .and_then(as_f64)
            }),
        "net_income_yoy" => fundamentals
            .pointer("/Financials/Income_Statement/quarterly")
            .and_then(|v| v.as_object())
            .and_then(|q| yoy_growth(q, "netIncome")),
        _ => None,
    }
}

#[derive(Debug)]
pub struct Options {
    pub symbols: Vec<String>,
    pub metrics: Vec<String>,
}

impl Options {
    pub fn parse(symbols: &str, metrics: &str) -> Result<Self, String> {
        let symbols: Vec<String> = symbols
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if symbols.is_empty() {
            return Err("symbols list is empty.".into());
        }
        if symbols.len() > MAX_SYMBOLS {
            return Err(format!(
                "too many symbols ({}); max {}.",
                symbols.len(),
                MAX_SYMBOLS
            ));
        }

        let metrics: Vec<String> = metrics
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        if metrics.is_empty() {
            return Err("metrics list is empty.".into());
        }
        let unknown: Vec<&str> = metrics
            .iter()
            .filter(|k| lookup_spec(k).is_none())
            .map(String::as_str)
            .collect();
        if !unknown.is_empty() {
            let known: Vec<&str> = METRIC_REGISTRY.iter().map(|m| m.key).collect();
            return Err(format!(
                "unknown metric(s): {}. Available: {}",
                unknown.join(", "),
                known.join(", ")
            ));
        }

        Ok(Self { symbols, metrics })
    }
}

pub async fn run(
    client: &EodhdClient,
    cache: &Cache,
    opts: Options,
    as_of: &str,
) -> Result<String, String> {
    // True parallelism: spawn one task per symbol with cloned client+cache
    // (both are Arc-backed and cheap to clone). Awaiting in input order
    // preserves output order without sacrificing concurrency — the runtime
    // already started polling every task.
    let handles: Vec<_> = opts
        .symbols
        .iter()
        .map(|sym| {
            let sym = sym.clone();
            let client = client.clone();
            let cache = cache.clone();
            tokio::spawn(async move {
                let result = fundamentals_trimmed(&client, &cache, &sym, FETCH_PERIODS).await;
                (sym, result)
            })
        })
        .collect();

    let mut results: Vec<(String, Result<Value, String>)> = Vec::with_capacity(handles.len());
    for h in handles {
        match h.await {
            Ok(pair) => results.push(pair),
            Err(e) => {
                // Spawned task panicked or was cancelled — surface as a fetch failure
                // for the symbol but don't abort the whole compare.
                results.push((
                    "<unknown>".into(),
                    Err(format!("internal task error: {}", e)),
                ));
            }
        }
    }

    let mut warnings: Vec<String> = Vec::new();
    let mut payloads: Vec<(String, Value)> = Vec::new();
    for (sym, res) in results {
        match res {
            Ok(v) => payloads.push((sym, v)),
            Err(e) => warnings.push(format!("{}: fetch failed — {}", sym, e)),
        }
    }
    if payloads.is_empty() {
        return Err("All fetches failed; nothing to compare.".into());
    }

    let ratios: Vec<(String, RatioSet, Value)> = payloads
        .into_iter()
        .map(|(s, v)| {
            let r = compute_ratios(&v);
            (s, r, v)
        })
        .collect();

    // Build value matrix: rows = metrics, columns = symbols
    let mut value_rows: Vec<(MetricSpec, Vec<Option<f64>>)> = Vec::new();
    for key in &opts.metrics {
        let spec = lookup_spec(key).expect("validated above");
        let row: Vec<Option<f64>> = ratios
            .iter()
            .map(|(_, r, v)| extract_metric(key, r, v))
            .collect();
        value_rows.push((MetricSpec { ..*spec }, row));
    }

    let value_table = render_value_table(&ratios, &value_rows);
    let rank_table = render_rank_table(&ratios, &value_rows);
    let summary = render_summary(&opts, &ratios);

    let data = format!(
        "### Values\n\n{}\n\n### Ranking (1 = best)\n\n{}",
        value_table, rank_table
    );

    let mut metadata = Metadata::new(as_of)
        .with_source("EODHD_fundamentals")
        .with_source("derived");
    for w in warnings {
        metadata = metadata.with_warning(w);
    }

    Ok(render_envelope(&summary, &data, &metadata))
}

fn render_value_table(
    ratios: &[(String, RatioSet, Value)],
    rows: &[(MetricSpec, Vec<Option<f64>>)],
) -> String {
    let mut out = String::new();
    out.push_str("| Metric |");
    for (sym, _, _) in ratios {
        out.push_str(&format!(" {} |", sym));
    }
    out.push('\n');
    out.push_str("| --- |");
    for _ in ratios {
        out.push_str(" ---: |");
    }
    out.push('\n');

    for (spec, vals) in rows {
        out.push_str(&format!("| {} |", spec.label));
        for v in vals {
            out.push_str(&format!(" {} |", format_value(*v, spec.format)));
        }
        out.push('\n');
    }
    out
}

fn render_rank_table(
    ratios: &[(String, RatioSet, Value)],
    rows: &[(MetricSpec, Vec<Option<f64>>)],
) -> String {
    let mut out = String::new();
    out.push_str("| Metric |");
    for (sym, _, _) in ratios {
        out.push_str(&format!(" {} |", sym));
    }
    out.push('\n');
    out.push_str("| --- |");
    for _ in ratios {
        out.push_str(" ---: |");
    }
    out.push('\n');

    for (spec, vals) in rows {
        out.push_str(&format!("| {} |", spec.label));
        let ranks = compute_ranks(vals, spec.direction);
        for r in ranks {
            out.push_str(&format!(
                " {} |",
                r.map(|n| n.to_string()).unwrap_or_else(|| "—".to_string())
            ));
        }
        out.push('\n');
    }
    out
}

/// 1-indexed rank; ties share the same rank; missing values are not ranked.
fn compute_ranks(vals: &[Option<f64>], direction: Direction) -> Vec<Option<usize>> {
    let mut indexed: Vec<(usize, f64)> = vals
        .iter()
        .enumerate()
        .filter_map(|(i, v)| v.map(|x| (i, x)))
        .collect();
    indexed.sort_by(|a, b| match direction {
        Direction::Higher => b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal),
        Direction::Lower => a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal),
    });

    let mut ranks: Vec<Option<usize>> = vec![None; vals.len()];
    let mut prev_value: Option<f64> = None;
    let mut prev_rank: usize = 0;
    for (rank_pos, (orig_idx, val)) in indexed.iter().enumerate() {
        let rank = if let Some(prev) = prev_value {
            if (prev - val).abs() < 1e-12 {
                prev_rank
            } else {
                rank_pos + 1
            }
        } else {
            rank_pos + 1
        };
        ranks[*orig_idx] = Some(rank);
        prev_value = Some(*val);
        prev_rank = rank;
    }
    ranks
}

fn format_value(v: Option<f64>, format: Format) -> String {
    match v {
        None => "—".to_string(),
        Some(x) => match format {
            Format::Percent => format!("{:.2}%", x * 100.0),
            Format::Ratio => format!("{:.2}", x),
        },
    }
}

fn render_summary(opts: &Options, ratios: &[(String, RatioSet, Value)]) -> String {
    let names: Vec<String> = ratios
        .iter()
        .map(|(s, _, v)| {
            let nm = v
                .pointer("/General/Name")
                .and_then(|x| x.as_str())
                .unwrap_or(s);
            format!("{} ({})", nm, s)
        })
        .collect();
    format!(
        "Comparison across {} on {} metric{}. Lower-is-better metrics (debt, valuation multiples) and higher-is-better metrics (margins, growth) are ranked separately in the ranking table.",
        names.join(", "),
        opts.metrics.len(),
        if opts.metrics.len() == 1 { "" } else { "s" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_too_many_symbols() {
        let r = Options::parse("A,B,C,D,E,F", "pe");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("max"));
    }

    #[test]
    fn parse_rejects_unknown_metric() {
        let r = Options::parse("AAPL.US", "foo,pe");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("unknown metric"));
    }

    #[test]
    fn parse_strips_whitespace_and_lowercases_metrics() {
        let opts = Options::parse(" AAPL.US , MSFT.US ", " PE , GROSS_MARGIN ").unwrap();
        assert_eq!(opts.symbols, vec!["AAPL.US", "MSFT.US"]);
        assert_eq!(opts.metrics, vec!["pe", "gross_margin"]);
    }

    #[test]
    fn rank_higher_is_better_assigns_one_to_max() {
        let ranks = compute_ranks(
            &[Some(10.0), Some(50.0), Some(20.0), None],
            Direction::Higher,
        );
        assert_eq!(ranks, vec![Some(3), Some(1), Some(2), None]);
    }

    #[test]
    fn rank_lower_is_better_assigns_one_to_min() {
        let ranks = compute_ranks(
            &[Some(10.0), Some(50.0), Some(20.0)],
            Direction::Lower,
        );
        assert_eq!(ranks, vec![Some(1), Some(3), Some(2)]);
    }

    #[test]
    fn rank_handles_ties() {
        let ranks = compute_ranks(
            &[Some(10.0), Some(10.0), Some(20.0)],
            Direction::Lower,
        );
        // Tie at the top → both rank 1; next gets 3 (skipped 2 — standard
        // competition ranking).
        assert_eq!(ranks, vec![Some(1), Some(1), Some(3)]);
    }

    #[test]
    fn extract_metric_pulls_from_ratio_set_and_growth() {
        let raw = include_str!("../../tests/fixtures/aapl_fundamentals.json");
        let v: Value = serde_json::from_str(raw).unwrap();
        let r = compute_ratios(&v);

        let pe = extract_metric("pe", &r, &v).unwrap();
        assert!((pe - 32.5).abs() < 0.001);

        let gm = extract_metric("gross_margin", &r, &v).unwrap();
        assert!((gm - 0.4638).abs() < 0.01);

        // Growth pulled from quarterly periods directly
        let yoy = extract_metric("revenue_yoy", &r, &v).unwrap();
        assert!((yoy - 0.0395).abs() < 0.01);

        assert!(extract_metric("nonsense", &r, &v).is_none());
    }
}
