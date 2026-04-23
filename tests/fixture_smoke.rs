//! Sanity-check that the fixture loader wires correctly and the AAPL fixture
//! has the structure the analytics modules will rely on.

mod common;

use common::load_fixture;

#[test]
fn aapl_fixture_has_expected_top_level_sections() {
    let v = load_fixture("aapl_fundamentals");
    for section in [
        "General",
        "Highlights",
        "Valuation",
        "SharesStats",
        "Financials",
    ] {
        assert!(
            v.get(section).is_some(),
            "fixture missing top-level section '{}'",
            section
        );
    }
}

#[test]
fn aapl_fixture_has_eight_quarters_of_income_statement() {
    let v = load_fixture("aapl_fundamentals");
    let quarters = v["Financials"]["Income_Statement"]["quarterly"]
        .as_object()
        .expect("quarterly should be a JSON object keyed by date");
    assert_eq!(
        quarters.len(),
        8,
        "expected 8 quarterly periods, got {}",
        quarters.len()
    );
}

#[test]
fn aapl_fixture_contains_a_non_recurring_other_non_cash_spike() {
    // This is the synthetic spike at 2024-09-30 used by anomaly tests.
    let v = load_fixture("aapl_fundamentals");
    let q = &v["Financials"]["Cash_Flow"]["quarterly"]["2024-09-30"]["otherNonCashItems"];
    assert_eq!(q.as_str(), Some("10520000000"));
}

#[test]
fn aapl_fixture_contains_nulls_in_older_quarters() {
    // Problem #7 in spec — nulls in older quarters; analytics must tolerate them.
    let v = load_fixture("aapl_fundamentals");
    let q = &v["Financials"]["Cash_Flow"]["quarterly"]["2023-03-31"]["stockBasedCompensation"];
    assert!(q.is_null());
}
