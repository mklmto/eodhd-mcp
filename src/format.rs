use serde_json::Value;
use std::collections::BTreeSet;

const MAX_TABLE_ROWS: usize = 150;
const MAX_COL_WIDTH: usize = 40;

/// Metadata block that accompanies any structured tool response. Mirrors
/// spec §5.5 — every new capability tool wraps its body in
/// `<summary>/<data>/<metadata>` so the consuming LLM gets a uniform
/// surface for prose, structured data, and provenance.
#[derive(Debug, Clone)]
pub struct Metadata {
    /// ISO date the response was assembled. Always populated.
    pub as_of: String,
    /// Optional human-readable freshness hint, e.g.
    /// `"last_quarter=2024-12-31 (filing_age=112d)"`.
    pub data_freshness: Option<String>,
    /// Non-fatal warnings the tool wants to surface (missing periods,
    /// detected anomalies, sentinel coverage gaps). Empty list rendered
    /// as `none`.
    pub warnings: Vec<String>,
    /// Provenance identifiers, e.g. `["EODHD_fundamentals", "derived"]`.
    pub sources: Vec<String>,
    /// Optional `cache_hit=true|false` — surfaced for observability.
    pub cache_hit: Option<bool>,
}

impl Metadata {
    pub fn new(as_of: impl Into<String>) -> Self {
        Self {
            as_of: as_of.into(),
            data_freshness: None,
            warnings: Vec::new(),
            sources: Vec::new(),
            cache_hit: None,
        }
    }

    pub fn with_freshness(mut self, f: impl Into<String>) -> Self {
        self.data_freshness = Some(f.into());
        self
    }

    pub fn with_source(mut self, s: impl Into<String>) -> Self {
        self.sources.push(s.into());
        self
    }

    pub fn with_warning(mut self, w: impl Into<String>) -> Self {
        self.warnings.push(w.into());
        self
    }

    pub fn with_cache_hit(mut self, hit: bool) -> Self {
        self.cache_hit = Some(hit);
        self
    }
}

/// Wrap a tool response in the spec §5.5 three-block envelope. `data` is
/// already formatted (markdown table, JSON, etc.) — this helper does not
/// re-format it. Tags are HTML-style as the spec specifies; the body is
/// markdown so Claude renders sections cleanly.
pub fn render_envelope(summary: &str, data: &str, metadata: &Metadata) -> String {
    let mut out = String::with_capacity(summary.len() + data.len() + 256);
    out.push_str("<summary>\n");
    out.push_str(summary.trim());
    out.push_str("\n</summary>\n\n");

    out.push_str("<data>\n");
    out.push_str(data.trim_end());
    out.push_str("\n</data>\n\n");

    out.push_str("<metadata>\n");
    out.push_str(&format!("- as_of: {}\n", metadata.as_of));
    if let Some(ref f) = metadata.data_freshness {
        out.push_str(&format!("- data_freshness: {}\n", f));
    }
    if metadata.warnings.is_empty() {
        out.push_str("- warnings: none\n");
    } else {
        out.push_str("- warnings:\n");
        for w in &metadata.warnings {
            out.push_str(&format!("  - {}\n", w));
        }
    }
    if !metadata.sources.is_empty() {
        out.push_str(&format!("- sources: [{}]\n", metadata.sources.join(", ")));
    }
    if let Some(hit) = metadata.cache_hit {
        out.push_str(&format!("- cache_hit: {}\n", hit));
    }
    out.push_str("</metadata>\n");
    out
}

/// Format a JSON value as hybrid output: markdown tables for flat arrays, JSON for nested data.
pub fn format_value(label: &str, data: &Value) -> String {
    match data {
        Value::Array(arr) if !arr.is_empty() && arr.iter().all(|v| v.is_object()) => {
            format_object_array(label, arr)
        }
        Value::Array(arr) if arr.is_empty() => {
            format!("**{}**\n\nNo results found.", label)
        }
        Value::Object(map) if map.len() <= 20 && map.values().all(is_scalar) => {
            format_flat_object(label, data)
        }
        _ => format_as_json(label, data),
    }
}

fn is_scalar(v: &Value) -> bool {
    matches!(
        v,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

/// Format an array of objects as a markdown table.
fn format_object_array(label: &str, arr: &[Value]) -> String {
    let truncated = arr.len() > MAX_TABLE_ROWS;
    let items = if truncated {
        &arr[..MAX_TABLE_ROWS]
    } else {
        arr
    };

    // Collect all keys in order
    let mut keys = BTreeSet::new();
    for item in items {
        if let Value::Object(map) = item {
            for key in map.keys() {
                keys.insert(key.clone());
            }
        }
    }
    let keys: Vec<String> = keys.into_iter().collect();

    if keys.is_empty() {
        return format!("**{}**\n\nEmpty results.", label);
    }

    let mut out = String::new();
    out.push_str(&format!(
        "**{}** ({} row{}{})\n\n",
        label,
        arr.len(),
        if arr.len() == 1 { "" } else { "s" },
        if truncated {
            format!(", showing first {}", MAX_TABLE_ROWS)
        } else {
            String::new()
        }
    ));

    // Header
    out.push('|');
    for key in &keys {
        out.push_str(&format!(" {} |", truncate_str(key, MAX_COL_WIDTH)));
    }
    out.push('\n');

    // Separator
    out.push('|');
    for _ in &keys {
        out.push_str(" --- |");
    }
    out.push('\n');

    // Rows
    for item in items {
        out.push('|');
        if let Value::Object(map) = item {
            for key in &keys {
                let cell = match map.get(key) {
                    Some(Value::String(s)) => truncate_str(s, MAX_COL_WIDTH).to_string(),
                    Some(Value::Null) => "—".to_string(),
                    Some(v) => truncate_str(&v.to_string(), MAX_COL_WIDTH).to_string(),
                    None => "—".to_string(),
                };
                out.push_str(&format!(" {} |", cell));
            }
        }
        out.push('\n');
    }

    if truncated {
        out.push_str(&format!(
            "\n*... {} more rows not shown.*\n",
            arr.len() - MAX_TABLE_ROWS
        ));
    }

    out
}

/// Format a flat object as a vertical key-value markdown table.
fn format_flat_object(label: &str, data: &Value) -> String {
    let mut out = String::new();
    out.push_str(&format!("**{}**\n\n", label));
    out.push_str("| Field | Value |\n| --- | --- |\n");

    if let Value::Object(map) = data {
        for (key, val) in map {
            let display = match val {
                Value::String(s) => s.clone(),
                Value::Null => "—".to_string(),
                v => v.to_string(),
            };
            out.push_str(&format!("| {} | {} |\n", key, truncate_str(&display, 80)));
        }
    }
    out
}

/// Format complex/nested data as pretty-printed JSON with a label.
fn format_as_json(label: &str, data: &Value) -> String {
    let json = serde_json::to_string_pretty(data).unwrap_or_else(|_| data.to_string());
    let truncated = json.len() > 50_000;
    let display = if truncated { &json[..50_000] } else { &json };
    let mut out = format!("**{}**\n\n```json\n{}", label, display);
    if truncated {
        out.push_str("\n... (truncated, data too large)");
    }
    out.push_str("\n```\n");
    out
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_contains_all_three_sections() {
        let md = Metadata::new("2026-04-22")
            .with_freshness("last_quarter=2024-12-31 (filing_age=112d)")
            .with_source("EODHD_fundamentals")
            .with_source("derived")
            .with_warning("Q4'23 totalRevenue is null in source")
            .with_cache_hit(true);
        let out = render_envelope("Apple grew revenue 6%.", "**TABLE HERE**", &md);

        assert!(out.contains("<summary>"));
        assert!(out.contains("Apple grew revenue 6%."));
        assert!(out.contains("</summary>"));
        assert!(out.contains("<data>"));
        assert!(out.contains("**TABLE HERE**"));
        assert!(out.contains("</data>"));
        assert!(out.contains("<metadata>"));
        assert!(out.contains("- as_of: 2026-04-22"));
        assert!(out.contains("- data_freshness:"));
        assert!(out.contains("Q4'23 totalRevenue is null"));
        assert!(out.contains("- sources: [EODHD_fundamentals, derived]"));
        assert!(out.contains("- cache_hit: true"));
        assert!(out.contains("</metadata>"));
    }

    #[test]
    fn envelope_warnings_block_says_none_when_empty() {
        let md = Metadata::new("2026-04-22");
        let out = render_envelope("ok", "data", &md);
        assert!(out.contains("- warnings: none"));
    }
}
