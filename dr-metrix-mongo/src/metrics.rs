
use async_trait::async_trait;
use bson::doc;
use mongodb::Client;
use prometheus::*;

use dr_metrix_core::collector::CollectorConfig;
use dr_metrix_core::error::{MetricsError, Result};
use dr_metrix_core::MetricsCollector;

#[derive(Clone)]
pub struct MongoMetrics {
    client: Client,
    admin_db: String,

    // Connection pool
    connections_current: IntGauge,
    connections_available: IntGauge,
    connections_total: IntGauge,

    // Opcounters (absolute snapshots)
    opcounters: IntGaugeVec,

    // Network
    bytes_in: IntGauge,
    bytes_out: IntGauge,
    requests_in: IntGauge,

    // Memory (MB → bytes)
    resident_memory_bytes: IntGauge,
    virtual_memory_bytes: IntGauge,

    // WiredTiger cache
    wt_cache_bytes_in_use: IntGauge,
    wt_cache_bytes_max: IntGauge,
    wt_cache_dirty_bytes: IntGauge,

    // App-level
    pub op_duration_seconds: HistogramVec,
}

impl MongoMetrics {
    pub fn new(client: Client, admin_db: &str, config: CollectorConfig) -> Result<Self> {
        let ns = &config.namespace;
        let sub = "mongo";

        macro_rules! gauge {
            ($name:expr, $help:expr) => {{
                let mut opts = Opts::new($name, $help).subsystem(sub);
                if !ns.is_empty() {
                    opts = opts.namespace(ns.as_str());
                }
                IntGauge::with_opts(opts)?
            }};
        }

        macro_rules! gauge_vec {
            ($name:expr, $help:expr, $labels:expr) => {{
                let mut opts = Opts::new($name, $help).subsystem(sub);
                if !ns.is_empty() {
                    opts = opts.namespace(ns.as_str());
                }
                IntGaugeVec::new(opts, $labels)?
            }};
        }

        macro_rules! histogram_vec {
            ($name:expr, $help:expr, $labels:expr, $buckets:expr) => {{
                let mut opts = HistogramOpts::new($name, $help)
                    .subsystem(sub)
                    .buckets($buckets);
                if !ns.is_empty() {
                    opts = opts.namespace(ns.as_str());
                }
                HistogramVec::new(opts, $labels)?
            }};
        }

        Ok(Self {
            client,
            admin_db: admin_db.to_string(),
            connections_current: gauge!("connections_current", "Current MongoDB connections"),
            connections_available: gauge!(
                "connections_available",
                "Available MongoDB connections"
            ),
            connections_total: gauge!("connections_total", "Total MongoDB connections created"),
            opcounters: gauge_vec!(
                "opcounters_total",
                "MongoDB operation counters (snapshot)",
                &["op"]
            ),
            bytes_in: gauge!("network_bytes_in_total", "Network bytes received"),
            bytes_out: gauge!("network_bytes_out_total", "Network bytes sent"),
            requests_in: gauge!("network_requests_total", "Network requests received"),
            resident_memory_bytes: gauge!(
                "resident_memory_bytes",
                "Resident memory usage in bytes"
            ),
            virtual_memory_bytes: gauge!("virtual_memory_bytes", "Virtual memory usage in bytes"),
            wt_cache_bytes_in_use: gauge!(
                "wt_cache_bytes_in_use",
                "WiredTiger cache bytes currently in use"
            ),
            wt_cache_bytes_max: gauge!("wt_cache_bytes_max", "WiredTiger cache maximum bytes"),
            wt_cache_dirty_bytes: gauge!(
                "wt_cache_dirty_bytes",
                "WiredTiger cache dirty bytes"
            ),
            op_duration_seconds: histogram_vec!(
                "op_duration_seconds",
                "Application-level MongoDB operation duration",
                &["op"],
                dr_metrix_core::DEFAULT_QUERY_BUCKETS.to_vec()
            ),
        })
    }

    pub fn observe_op(&self, op: &str, duration_secs: f64) {
        self.op_duration_seconds
            .with_label_values(&[op])
            .observe(duration_secs);
    }
}

#[async_trait]
impl MetricsCollector for MongoMetrics {
    fn name(&self) -> &'static str {
        "mongo"
    }

    fn register(&self, registry: &Registry) -> Result<()> {
        registry.register(Box::new(self.connections_current.clone()))?;
        registry.register(Box::new(self.connections_available.clone()))?;
        registry.register(Box::new(self.connections_total.clone()))?;
        registry.register(Box::new(self.opcounters.clone()))?;
        registry.register(Box::new(self.bytes_in.clone()))?;
        registry.register(Box::new(self.bytes_out.clone()))?;
        registry.register(Box::new(self.requests_in.clone()))?;
        registry.register(Box::new(self.resident_memory_bytes.clone()))?;
        registry.register(Box::new(self.virtual_memory_bytes.clone()))?;
        registry.register(Box::new(self.wt_cache_bytes_in_use.clone()))?;
        registry.register(Box::new(self.wt_cache_bytes_max.clone()))?;
        registry.register(Box::new(self.wt_cache_dirty_bytes.clone()))?;
        registry.register(Box::new(self.op_duration_seconds.clone()))?;
        Ok(())
    }

    async fn collect(&self) -> Result<()> {
        let db = self.client.database(&self.admin_db);
        let doc = db
            .run_command(doc! { "serverStatus": 1 })
            .await
            .map_err(MetricsError::database)?;

        // Connections
        if let Ok(conns) = doc.get_document("connections") {
            self.connections_current
                .set(conns.get_i64("current").unwrap_or(0));
            self.connections_available
                .set(conns.get_i64("available").unwrap_or(0));
            self.connections_total
                .set(conns.get_i64("totalCreated").unwrap_or(0));
        }

        // Opcounters
        if let Ok(ops) = doc.get_document("opcounters") {
            for op in &["insert", "query", "update", "delete", "getmore", "command"] {
                let val = ops.get_i64(op).unwrap_or(0);
                self.opcounters.with_label_values(&[op]).set(val);
            }
        }

        // Network
        if let Ok(net) = doc.get_document("network") {
            self.bytes_in.set(net.get_i64("bytesIn").unwrap_or(0));
            self.bytes_out.set(net.get_i64("bytesOut").unwrap_or(0));
            self.requests_in
                .set(net.get_i64("numRequests").unwrap_or(0));
        }

        // Memory (MongoDB reports in MB)
        if let Ok(mem) = doc.get_document("mem") {
            self.resident_memory_bytes
                .set(mem.get_i64("resident").unwrap_or(0) * 1024 * 1024);
            self.virtual_memory_bytes
                .set(mem.get_i64("virtual").unwrap_or(0) * 1024 * 1024);
        }

        // WiredTiger cache
        if let Ok(wt) = doc.get_document("wiredTiger") {
            if let Ok(cache) = wt.get_document("cache") {
                self.wt_cache_bytes_in_use.set(
                    cache
                        .get_i64("bytes currently in the cache")
                        .unwrap_or(0),
                );
                self.wt_cache_bytes_max.set(
                    cache
                        .get_i64("maximum bytes configured")
                        .unwrap_or(0),
                );
                self.wt_cache_dirty_bytes.set(
                    cache
                        .get_i64("tracked dirty bytes in the cache")
                        .unwrap_or(0),
                );
            }
        }

        Ok(())
    }
}
