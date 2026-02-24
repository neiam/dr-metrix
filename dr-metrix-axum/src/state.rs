//! Shared application state that ties the Prometheus registry, HTTP metrics,
//! database collectors, and their background tasks together.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use prometheus::Registry;
use tokio::task::JoinHandle;

use dr_metrix_core::error::Result;
use dr_metrix_core::MetricsCollector;

use crate::http_metrics::HttpMetrics;
use crate::layer::HttpMetricsLayer;

/// Central Prometheus state for an Axum application.
///
/// Owns the registry, the HTTP-level metrics, and the background collector
/// task handles. Intended to be wrapped in `Arc` and used as Axum `State`.
pub struct PrometheusMetrics {
    /// The shared Prometheus registry. All collectors register here.
    pub registry: Registry,
    /// HTTP request/response metrics (used by the layer).
    pub http_metrics: Arc<HttpMetrics>,
    /// Background collector task handles for graceful shutdown.
    handles: std::sync::Mutex<Vec<(&'static str, JoinHandle<()>)>>,
    /// Number of registered collectors.
    collector_count: AtomicUsize,
}

impl PrometheusMetrics {
    /// Start building with a namespace prefix.
    pub fn builder(namespace: &str) -> PrometheusMetricsBuilder {
        PrometheusMetricsBuilder {
            namespace: namespace.to_string(),
            process_collector: false,
        }
    }

    /// Return a Tower layer that records HTTP metrics on every request.
    pub fn http_layer(&self) -> HttpMetricsLayer {
        HttpMetricsLayer::new(self.http_metrics.clone())
    }

    /// Register a [`MetricsCollector`] and spawn its background loop.
    ///
    /// The collector's metrics are added to the shared registry, and a
    /// background task calls `collect()` on the given interval.
    pub fn add_collector<C: MetricsCollector>(
        &self,
        collector: C,
        interval: Duration,
    ) -> Result<()> {
        let name = collector.name();
        collector.register(&self.registry)?;
        let arc = Arc::new(collector);
        let handle = arc.spawn_collect_loop(interval);

        self.handles
            .lock()
            .unwrap()
            .push((name, handle));
        self.collector_count.fetch_add(1, Ordering::Relaxed);

        tracing::info!(collector = name, ?interval, "collector registered and running");
        Ok(())
    }

    /// Number of active collectors.
    pub fn collector_count(&self) -> usize {
        self.collector_count.load(Ordering::Relaxed)
    }

    /// Gracefully abort all background collector tasks.
    pub fn shutdown(&self) {
        let mut handles = self.handles.lock().unwrap();
        for (name, handle) in handles.drain(..) {
            tracing::info!(collector = name, "shutting down collector");
            handle.abort();
        }
    }
}

impl Drop for PrometheusMetrics {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ───────────────────────────── Builder ───────────────────────────

pub struct PrometheusMetricsBuilder {
    namespace: String,
    process_collector: bool,
}

impl PrometheusMetricsBuilder {
    /// Register the default Prometheus **process collector** which exposes:
    /// `process_cpu_seconds_total`, `process_open_fds`,
    /// `process_max_fds`, `process_virtual_memory_bytes`,
    /// `process_virtual_memory_max_bytes`, `process_resident_memory_bytes`,
    /// `process_start_time_seconds`, `process_threads`.
    pub fn with_process_collector(mut self) -> Self {
        self.process_collector = true;
        self
    }

    pub fn build(self) -> Result<PrometheusMetrics> {
        let registry = Registry::new();

        // ── Process collector ──────────────────────────────────
        if self.process_collector {
            let pc = prometheus::process_collector::ProcessCollector::for_self();
            registry
                .register(Box::new(pc))
                .map_err(dr_metrix_core::MetricsError::Prometheus)?;
            tracing::info!("process collector registered");
        }

        // ── HTTP metrics ───────────────────────────────────────
        let http_metrics = Arc::new(HttpMetrics::new(&self.namespace)?);
        http_metrics.register(&registry)?;

        Ok(PrometheusMetrics {
            registry,
            http_metrics,
            handles: std::sync::Mutex::new(Vec::new()),
            collector_count: AtomicUsize::new(0),
        })
    }
}
