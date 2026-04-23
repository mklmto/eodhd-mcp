//! Higher-level "capability" tools that compose the raw EODHD client and
//! the analytics layer into LLM-friendly responses (spec §5.1):
//! `snapshot`, `financials`, `compare`, `health_check`.

pub mod compare;
pub mod fetch;
pub mod financials;
pub mod health_check;
pub mod snapshot;
