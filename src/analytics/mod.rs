//! Transformation layer that turns raw EODHD JSON into derived analytics:
//! ratios, TTM rollups, normalization, and (Phase 3) anomaly detection.
//!
//! Modules here are kept dependency-free against `client`/`server` — they
//! operate on `serde_json::Value` so the same code runs in both production
//! and unit tests without HTTP plumbing.

pub mod anomaly;
pub mod normalization;
pub mod ratios;
pub mod ttm;

pub use normalization::slice_periodic;
