use serde_json::Value;
use std::collections::BTreeSet;

const MAX_TABLE_ROWS: usize = 150;
const MAX_COL_WIDTH: usize = 40;

/// Format a JSON value as hybrid output: markdown tables for flat arrays, JSON for nested data.
pub fn format_value(label: &str, data: &Value) -> String {
    match data {
        Value::Array(arr) if !arr.is_empty() && arr.iter().all(|v| v.is_object()) => {
            format_object_array(label, arr)
        }
        Value::Array(arr) if arr.is_empty() => {
            format!("**{}**\n\nNo results found.", label)
        }
        Value::Object(map) if map.len() <= 20 && map.values().all(|v| is_scalar(v)) => {
            format_flat_object(label, data)
        }
        _ => format_as_json(label, data),
    }
}

fn is_scalar(v: &Value) -> bool {
    matches!(v, Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_))
}

/// Format an array of objects as a markdown table.
fn format_object_array(label: &str, arr: &[Value]) -> String {
    let truncated = arr.len() > MAX_TABLE_ROWS;
    let items = if truncated { &arr[..MAX_TABLE_ROWS] } else { arr };

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
    out.push_str(&format!("**{}** ({} row{}{})\n\n", label, arr.len(),
        if arr.len() == 1 { "" } else { "s" },
        if truncated { format!(", showing first {}", MAX_TABLE_ROWS) } else { String::new() }
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
        out.push_str(&format!("\n*... {} more rows not shown.*\n", arr.len() - MAX_TABLE_ROWS));
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
