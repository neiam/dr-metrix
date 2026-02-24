use std::sync::Arc;

use async_trait::async_trait;
use prometheus::*;
use serde_json::Value;

use dr_metrix_core::collector::CollectorConfig;
use dr_metrix_core::error::Result;
use dr_metrix_core::MetricsCollector;

use crate::pool::PoolAdapter;

const SQL_DB_STATS: &str = "
SELECT datname, numbackends, xact_commit, xact_rollback,
       blks_hit, blks_read, pg_database_size(datname) AS size_bytes
FROM pg_stat_database WHERE datname IS NOT NULL";

const SQL_TABLE_STATS: &str = "
SELECT schemaname, relname, seq_scan, idx_scan, n_live_tup, n_dead_tup
FROM pg_stat_user_tables";

const SQL_LOCKS: &str = "
SELECT mode, COUNT(*) AS cnt FROM pg_locks WHERE granted = true GROUP BY mode";

pub struct PostgresMetrics<P: PoolAdapter> {
    pool: Arc<P>,

    // Pool gauges
    pool_active: IntGauge,
    pool_idle: IntGauge,
    pool_max: IntGauge,

    // pg_stat_database
    db_numbackends: IntGaugeVec,
    db_xact_commit: IntGaugeVec,
    db_xact_rollback: IntGaugeVec,
    db_blks_hit: IntGaugeVec,
    db_blks_read: IntGaugeVec,
    db_size_bytes: IntGaugeVec,

    // pg_stat_user_tables
    table_seq_scan: IntGaugeVec,
    table_idx_scan: IntGaugeVec,
    table_live_tup: IntGaugeVec,
    table_dead_tup: IntGaugeVec,

    // pg_locks
    lock_count: IntGaugeVec,

    // App-level
    pub query_duration_seconds: HistogramVec,
    pub rows_returned: HistogramVec,
}

// Manual Clone so we don't require P: Clone
impl<P: PoolAdapter> Clone for PostgresMetrics<P> {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            pool_active: self.pool_active.clone(),
            pool_idle: self.pool_idle.clone(),
            pool_max: self.pool_max.clone(),
            db_numbackends: self.db_numbackends.clone(),
            db_xact_commit: self.db_xact_commit.clone(),
            db_xact_rollback: self.db_xact_rollback.clone(),
            db_blks_hit: self.db_blks_hit.clone(),
            db_blks_read: self.db_blks_read.clone(),
            db_size_bytes: self.db_size_bytes.clone(),
            table_seq_scan: self.table_seq_scan.clone(),
            table_idx_scan: self.table_idx_scan.clone(),
            table_live_tup: self.table_live_tup.clone(),
            table_dead_tup: self.table_dead_tup.clone(),
            lock_count: self.lock_count.clone(),
            query_duration_seconds: self.query_duration_seconds.clone(),
            rows_returned: self.rows_returned.clone(),
        }
    }
}

impl<P: PoolAdapter> PostgresMetrics<P> {
    pub fn new(pool: P, config: CollectorConfig) -> Result<Self> {
        let ns = &config.namespace;
        let sub = "postgres";

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
            pool: Arc::new(pool),
            pool_active: gauge!("pool_active", "Active pool connections"),
            pool_idle: gauge!("pool_idle", "Idle pool connections"),
            pool_max: gauge!("pool_max", "Maximum pool size"),
            db_numbackends: gauge_vec!(
                "db_numbackends",
                "Active backends per database",
                &["database"]
            ),
            db_xact_commit: gauge_vec!(
                "db_xact_commit_total",
                "Committed transactions (snapshot)",
                &["database"]
            ),
            db_xact_rollback: gauge_vec!(
                "db_xact_rollback_total",
                "Rolled-back transactions (snapshot)",
                &["database"]
            ),
            db_blks_hit: gauge_vec!(
                "db_blks_hit_total",
                "Buffer hits (snapshot)",
                &["database"]
            ),
            db_blks_read: gauge_vec!(
                "db_blks_read_total",
                "Disk blocks read (snapshot)",
                &["database"]
            ),
            db_size_bytes: gauge_vec!("db_size_bytes", "Database size in bytes", &["database"]),
            table_seq_scan: gauge_vec!(
                "table_seq_scan_total",
                "Sequential scans (snapshot)",
                &["schema", "table"]
            ),
            table_idx_scan: gauge_vec!(
                "table_idx_scan_total",
                "Index scans (snapshot)",
                &["schema", "table"]
            ),
            table_live_tup: gauge_vec!(
                "table_live_tup",
                "Estimated live rows",
                &["schema", "table"]
            ),
            table_dead_tup: gauge_vec!(
                "table_dead_tup",
                "Estimated dead rows",
                &["schema", "table"]
            ),
            lock_count: gauge_vec!("lock_count", "Active locks by mode", &["mode"]),
            query_duration_seconds: histogram_vec!(
                "query_duration_seconds",
                "Application query duration",
                &["query"],
                dr_metrix_core::DEFAULT_QUERY_BUCKETS.to_vec()
            ),
            rows_returned: histogram_vec!(
                "rows_returned",
                "Rows returned per application query",
                &["query"],
                vec![1.0, 5.0, 10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0]
            ),
        })
    }

    pub fn observe_query(&self, name: &str, duration_secs: f64, row_count: f64) {
        self.query_duration_seconds
            .with_label_values(&[name])
            .observe(duration_secs);
        self.rows_returned
            .with_label_values(&[name])
            .observe(row_count);
    }
}

fn get_i64(row: &serde_json::Map<String, Value>, key: &str) -> i64 {
    row.get(key)
        .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
        .unwrap_or(0)
}

fn get_str<'a>(row: &'a serde_json::Map<String, Value>, key: &str) -> &'a str {
    row.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

#[async_trait]
impl<P: PoolAdapter> MetricsCollector for PostgresMetrics<P> {
    fn name(&self) -> &'static str {
        "postgres"
    }

    fn register(&self, registry: &Registry) -> Result<()> {
        registry.register(Box::new(self.pool_active.clone()))?;
        registry.register(Box::new(self.pool_idle.clone()))?;
        registry.register(Box::new(self.pool_max.clone()))?;
        registry.register(Box::new(self.db_numbackends.clone()))?;
        registry.register(Box::new(self.db_xact_commit.clone()))?;
        registry.register(Box::new(self.db_xact_rollback.clone()))?;
        registry.register(Box::new(self.db_blks_hit.clone()))?;
        registry.register(Box::new(self.db_blks_read.clone()))?;
        registry.register(Box::new(self.db_size_bytes.clone()))?;
        registry.register(Box::new(self.table_seq_scan.clone()))?;
        registry.register(Box::new(self.table_idx_scan.clone()))?;
        registry.register(Box::new(self.table_live_tup.clone()))?;
        registry.register(Box::new(self.table_dead_tup.clone()))?;
        registry.register(Box::new(self.lock_count.clone()))?;
        registry.register(Box::new(self.query_duration_seconds.clone()))?;
        registry.register(Box::new(self.rows_returned.clone()))?;
        Ok(())
    }

    async fn collect(&self) -> Result<()> {
        // Pool status
        let status = self.pool.pool_status();
        self.pool_active.set(status.in_use as i64);
        self.pool_idle.set(status.available as i64);
        self.pool_max.set(status.max_size as i64);

        // pg_stat_database
        let db_rows = self.pool.query_json(SQL_DB_STATS).await?;
        for row in &db_rows {
            let db = get_str(row, "datname");
            self.db_numbackends
                .with_label_values(&[db])
                .set(get_i64(row, "numbackends"));
            self.db_xact_commit
                .with_label_values(&[db])
                .set(get_i64(row, "xact_commit"));
            self.db_xact_rollback
                .with_label_values(&[db])
                .set(get_i64(row, "xact_rollback"));
            self.db_blks_hit
                .with_label_values(&[db])
                .set(get_i64(row, "blks_hit"));
            self.db_blks_read
                .with_label_values(&[db])
                .set(get_i64(row, "blks_read"));
            self.db_size_bytes
                .with_label_values(&[db])
                .set(get_i64(row, "size_bytes"));
        }

        // pg_stat_user_tables
        let table_rows = self.pool.query_json(SQL_TABLE_STATS).await?;
        for row in &table_rows {
            let schema = get_str(row, "schemaname");
            let table = get_str(row, "relname");
            self.table_seq_scan
                .with_label_values(&[schema, table])
                .set(get_i64(row, "seq_scan"));
            self.table_idx_scan
                .with_label_values(&[schema, table])
                .set(get_i64(row, "idx_scan"));
            self.table_live_tup
                .with_label_values(&[schema, table])
                .set(get_i64(row, "n_live_tup"));
            self.table_dead_tup
                .with_label_values(&[schema, table])
                .set(get_i64(row, "n_dead_tup"));
        }

        // pg_locks
        let lock_rows = self.pool.query_json(SQL_LOCKS).await?;
        for row in &lock_rows {
            let mode = get_str(row, "mode");
            self.lock_count
                .with_label_values(&[mode])
                .set(get_i64(row, "cnt"));
        }

        Ok(())
    }
}
