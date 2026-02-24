//! # metrics-axum
//!
//! Axum integration for the metrics-exporter workspace.
//!
//! Provides:
//! - **`PrometheusMetrics`** — shared state holding the registry, HTTP request
//!   metrics, and database collector handles. Drop-in for Axum's `State`.
//! - **`HttpMetricsLayer`** — a Tower layer / middleware that automatically
//!   records `http_requests_total`, `http_request_duration_seconds`, and
//!   `http_requests_in_flight` for every request.
//! - **`metrics_handler`** — an Axum handler that encodes and serves the
//!   Prometheus text exposition format on `GET /metrics`.
//! - **`health_handler`** — a simple `GET /health` liveness probe.
//! - Helpers to register the default Prometheus **process collector** and an
//!   optional **Tokio runtime collector** so they appear alongside the
//!   database metrics.

pub mod handler;
pub mod http_metrics;
pub mod layer;
pub mod state;

pub use handler::{health_handler, metrics_handler};
pub use layer::HttpMetricsLayer;
pub use state::PrometheusMetrics;
