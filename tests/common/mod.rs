//! Shared test helpers. Loaded by integration tests via `mod common;`.

use serde_json::Value;
use std::path::PathBuf;

/// Load a JSON fixture from `tests/fixtures/{name}.json`.
///
/// Panics with a descriptive message if the file is missing or unparseable —
/// fixtures are part of the test contract, so a missing one is a hard error.
pub fn load_fixture(name: &str) -> Value {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("fixtures");
    path.push(format!("{}.json", name));

    let contents = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {}", path.display(), e));
    serde_json::from_str(&contents)
        .unwrap_or_else(|e| panic!("failed to parse fixture {}: {}", path.display(), e))
}
