use async_trait::async_trait;
use fred::clients::Pool as RedisPool;
use fred::interfaces::ClientLike;
use fred::types::InfoKind;
use prometheus::*;

use dr_metrix_core::collector::CollectorConfig;
use dr_metrix_core::error::{MetricsError, Result};
use dr_metrix_core::MetricsCollector;

use crate::info_parser;

#[derive(Clone)]
pub struct RedisMetrics {
    pool: RedisPool,

    // Pool
    pool_connections: IntGauge,

    // Clients
    connected_clients: IntGauge,
    blocked_clients: IntGauge,

    // Memory
    used_memory_bytes: IntGauge,
    used_memory_rss: IntGauge,
    mem_fragmentation_ratio: Gauge,

    // Stats (absolute snapshots from INFO)
    keyspace_hits: IntGauge,
    keyspace_misses: IntGauge,
    evicted_keys: IntGauge,
    total_commands_processed: IntGauge,

    // Keyspace
    db_keys: IntGaugeVec,
    db_expires: IntGaugeVec,

    // Replication
    replication_offset: IntGauge,
    connected_slaves: IntGauge,

    // App-level
    pub command_duration_seconds: HistogramVec,
}

impl RedisMetrics {
    pub fn new(pool: RedisPool, config: CollectorConfig) -> Result<Self> {
        let ns = &config.namespace;
        let sub = "redis";

        macro_rules! gauge {
            ($name:expr, $help:expr) => {{
                let mut opts = Opts::new($name, $help).subsystem(sub);
                if !ns.is_empty() {
                    opts = opts.namespace(ns.as_str());
                }
                IntGauge::with_opts(opts)?
            }};
        }

        macro_rules! fgauge {
            ($name:expr, $help:expr) => {{
                let mut opts = Opts::new($name, $help).subsystem(sub);
                if !ns.is_empty() {
                    opts = opts.namespace(ns.as_str());
                }
                Gauge::with_opts(opts)?
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
            pool,
            pool_connections: gauge!("pool_connections", "Number of connected pool clients"),
            connected_clients: gauge!("connected_clients", "Number of connected Redis clients"),
            blocked_clients: gauge!("blocked_clients", "Number of blocked Redis clients"),
            used_memory_bytes: gauge!("used_memory_bytes", "Redis used memory in bytes"),
            used_memory_rss: gauge!("used_memory_rss_bytes", "Redis RSS memory in bytes"),
            mem_fragmentation_ratio: fgauge!(
                "mem_fragmentation_ratio",
                "Redis memory fragmentation ratio"
            ),
            keyspace_hits: gauge!("keyspace_hits_total", "Redis keyspace hits (snapshot)"),
            keyspace_misses: gauge!(
                "keyspace_misses_total",
                "Redis keyspace misses (snapshot)"
            ),
            evicted_keys: gauge!("evicted_keys_total", "Redis evicted keys (snapshot)"),
            total_commands_processed: gauge!(
                "commands_processed_total",
                "Total Redis commands processed (snapshot)"
            ),
            db_keys: gauge_vec!("db_keys", "Number of keys per database", &["db"]),
            db_expires: gauge_vec!("db_expires", "Number of expiring keys per database", &["db"]),
            replication_offset: gauge!(
                "replication_offset",
                "Redis master replication offset"
            ),
            connected_slaves: gauge!("connected_slaves", "Number of connected Redis replicas"),
            command_duration_seconds: histogram_vec!(
                "command_duration_seconds",
                "Application-level Redis command duration",
                &["cmd"],
                dr_metrix_core::DEFAULT_QUERY_BUCKETS.to_vec()
            ),
        })
    }

    pub fn observe_command(&self, cmd: &str, duration_secs: f64) {
        self.command_duration_seconds
            .with_label_values(&[cmd])
            .observe(duration_secs);
    }
}

#[async_trait]
impl MetricsCollector for RedisMetrics {
    fn name(&self) -> &'static str {
        "redis"
    }

    fn register(&self, registry: &Registry) -> Result<()> {
        registry.register(Box::new(self.pool_connections.clone()))?;
        registry.register(Box::new(self.connected_clients.clone()))?;
        registry.register(Box::new(self.blocked_clients.clone()))?;
        registry.register(Box::new(self.used_memory_bytes.clone()))?;
        registry.register(Box::new(self.used_memory_rss.clone()))?;
        registry.register(Box::new(self.mem_fragmentation_ratio.clone()))?;
        registry.register(Box::new(self.keyspace_hits.clone()))?;
        registry.register(Box::new(self.keyspace_misses.clone()))?;
        registry.register(Box::new(self.evicted_keys.clone()))?;
        registry.register(Box::new(self.total_commands_processed.clone()))?;
        registry.register(Box::new(self.db_keys.clone()))?;
        registry.register(Box::new(self.db_expires.clone()))?;
        registry.register(Box::new(self.replication_offset.clone()))?;
        registry.register(Box::new(self.connected_slaves.clone()))?;
        registry.register(Box::new(self.command_duration_seconds.clone()))?;
        Ok(())
    }

    async fn collect(&self) -> Result<()> {
        // Pool connection count
        self.pool_connections
            .set(self.pool.size() as i64);

        // INFO ALL
        let raw: String = self
            .pool
            .info(Some(InfoKind::All))
            .await
            .map_err(MetricsError::database)?;

        let info = info_parser::parse_info(&raw);

        self.connected_clients.set(info.connected_clients);
        self.blocked_clients.set(info.blocked_clients);
        self.used_memory_bytes.set(info.used_memory);
        self.used_memory_rss.set(info.used_memory_rss);
        self.mem_fragmentation_ratio
            .set(info.mem_fragmentation_ratio);
        self.keyspace_hits.set(info.keyspace_hits);
        self.keyspace_misses.set(info.keyspace_misses);
        self.evicted_keys.set(info.evicted_keys);
        self.total_commands_processed
            .set(info.total_commands_processed);
        self.replication_offset.set(info.replication_offset);
        self.connected_slaves.set(info.connected_slaves);

        for (db_idx, keys, expires) in &info.keyspace {
            let db_label = format!("db{db_idx}");
            self.db_keys
                .with_label_values(&[&db_label])
                .set(*keys);
            self.db_expires
                .with_label_values(&[&db_label])
                .set(*expires);
        }

        Ok(())
    }
}
