//! Tower `Layer` + `Service` that records HTTP metrics for every request.
//!
//! This is a manual Tower implementation so it works with any Tower-compatible
//! framework and can accurately measure the full response lifecycle.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use axum::body::Body;
use axum::extract::MatchedPath;
use http::{Request, Response};
use tower::{Layer, Service};

use crate::http_metrics::HttpMetrics;

// ───────────────────────────── Layer ─────────────────────────────

/// A Tower [`Layer`] that wraps services with [`HttpMetricsService`].
///
/// Install on your Axum router via `.layer(prom.http_layer())`.
#[derive(Clone)]
pub struct HttpMetricsLayer {
    metrics: Arc<HttpMetrics>,
}

impl HttpMetricsLayer {
    pub fn new(metrics: Arc<HttpMetrics>) -> Self {
        Self { metrics }
    }
}

impl<S> Layer<S> for HttpMetricsLayer {
    type Service = HttpMetricsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        HttpMetricsService {
            inner,
            metrics: self.metrics.clone(),
        }
    }
}

// ───────────────────────────── Service ───────────────────────────

/// Tower [`Service`] that records request count, duration, in-flight gauge,
/// and response size for every HTTP request.
#[derive(Clone)]
pub struct HttpMetricsService<S> {
    inner: S,
    metrics: Arc<HttpMetrics>,
}

impl<S> Service<Request<Body>> for HttpMetricsService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: std::fmt::Display + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let metrics = self.metrics.clone();
        let method = req.method().to_string();

        // Use the Axum matched-path template if available (e.g. "/api/users/:id");
        // fall back to the raw URI path.
        let path = req
            .extensions()
            .get::<MatchedPath>()
            .map(|m| m.as_str().to_owned())
            .unwrap_or_else(|| req.uri().path().to_owned());

        metrics.requests_in_flight.inc();
        let start = Instant::now();

        // Clone inner service: `call` takes `&mut self` but we move into async.
        let mut svc = self.inner.clone();

        Box::pin(async move {
            let result = svc.call(req).await;
            let elapsed = start.elapsed().as_secs_f64();
            metrics.requests_in_flight.dec();

            match &result {
                Ok(response) => {
                    let status = response.status().as_u16().to_string();
                    metrics
                        .requests_total
                        .with_label_values(&[&method, &path, &status])
                        .inc();
                    metrics
                        .request_duration_seconds
                        .with_label_values(&[&method, &path, &status])
                        .observe(elapsed);

                    // Best-effort: use Content-Length header if present.
                    if let Some(len) = response
                        .headers()
                        .get(http::header::CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<f64>().ok())
                    {
                        metrics
                            .response_size_bytes
                            .with_label_values(&[&method, &path])
                            .observe(len);
                    }
                }
                Err(err) => {
                    tracing::debug!(error = %err, "request failed before response");
                    metrics
                        .requests_total
                        .with_label_values(&[&method, &path, "500"])
                        .inc();
                    metrics
                        .request_duration_seconds
                        .with_label_values(&[&method, &path, "500"])
                        .observe(elapsed);
                }
            }

            result
        })
    }
}
