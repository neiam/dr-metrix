//! Axum handlers for `/metrics` and `/health`.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use prometheus::{Encoder, TextEncoder};

use crate::state::PrometheusMetrics;

/// `GET /metrics` — renders all registered Prometheus metrics in text
/// exposition format. This includes:
/// - Default process metrics (pid, open FDs, virtual memory, CPU, start time)
/// - HTTP request metrics from the middleware layer
/// - All database collector metrics (postgres, redis, mongo)
pub async fn metrics_handler(
    State(prom): State<Arc<PrometheusMetrics>>,
) -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let families = prom.registry.gather();
    let mut buf = Vec::with_capacity(8192);

    match encoder.encode(&families, &mut buf) {
        Ok(()) => (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            buf,
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to encode metrics");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `GET /health` — returns 200 with the number of healthy collectors.
pub async fn health_handler(
    State(prom): State<Arc<PrometheusMetrics>>,
) -> impl IntoResponse {
    let active = prom.collector_count();
    (
        StatusCode::OK,
        format!("{{\"status\":\"ok\",\"collectors\":{active}}}"),
    )
}
