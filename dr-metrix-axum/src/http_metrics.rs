use prometheus::*;

use dr_metrix_core::error::Result;
use dr_metrix_core::DEFAULT_QUERY_BUCKETS;

/// HTTP-level Prometheus metrics recorded by the middleware layer.
#[derive(Clone)]
pub struct HttpMetrics {
    /// Counter: total HTTP requests, labelled by method, path, and status.
    pub requests_total: IntCounterVec,
    /// Histogram: request duration in seconds, labelled by method, path, and status.
    pub request_duration_seconds: HistogramVec,
    /// Gauge: currently in-flight requests.
    pub requests_in_flight: IntGauge,
    /// Histogram: response body size in bytes, labelled by method and path.
    pub response_size_bytes: HistogramVec,
}

impl HttpMetrics {
    pub fn new(namespace: &str) -> Result<Self> {
        let ns = if namespace.is_empty() {
            String::new()
        } else {
            namespace.to_string()
        };
        let sub = "http";

        let mut req_opts =
            Opts::new("requests_total", "Total HTTP requests").subsystem(sub);
        if !ns.is_empty() {
            req_opts = req_opts.namespace(ns.clone());
        }

        let mut dur_opts = HistogramOpts::new(
            "request_duration_seconds",
            "HTTP request latency in seconds",
        )
        .subsystem(sub)
        .buckets(DEFAULT_QUERY_BUCKETS.to_vec());
        if !ns.is_empty() {
            dur_opts = dur_opts.namespace(ns.clone());
        }

        let mut flight_opts =
            Opts::new("requests_in_flight", "Currently in-flight HTTP requests").subsystem(sub);
        if !ns.is_empty() {
            flight_opts = flight_opts.namespace(ns.clone());
        }

        let mut size_opts = HistogramOpts::new(
            "response_size_bytes",
            "HTTP response body size in bytes",
        )
        .subsystem(sub)
        .buckets(vec![
            100.0, 1_000.0, 10_000.0, 100_000.0, 1_000_000.0, 10_000_000.0,
        ]);
        if !ns.is_empty() {
            size_opts = size_opts.namespace(ns.clone());
        }

        Ok(Self {
            requests_total: IntCounterVec::new(req_opts, &["method", "path", "status"])?,
            request_duration_seconds: HistogramVec::new(dur_opts, &["method", "path", "status"])?,
            requests_in_flight: IntGauge::with_opts(flight_opts)?,
            response_size_bytes: HistogramVec::new(size_opts, &["method", "path"])?,
        })
    }

    pub fn register(&self, registry: &Registry) -> Result<()> {
        registry.register(Box::new(self.requests_total.clone()))?;
        registry.register(Box::new(self.request_duration_seconds.clone()))?;
        registry.register(Box::new(self.requests_in_flight.clone()))?;
        registry.register(Box::new(self.response_size_bytes.clone()))?;
        Ok(())
    }
}
