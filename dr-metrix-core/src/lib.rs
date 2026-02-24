pub mod collector;
pub mod error;

pub const DEFAULT_QUERY_BUCKETS: &[f64] = &[
    0.0001, 0.0005, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

pub use collector::{CollectorConfig, MetricsCollector};
pub use error::{MetricsError, Result};
