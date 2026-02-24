# Prometheus Database Metrics Exporters

A Rust workspace of five crates for exporting Prometheus metrics from PostgreSQL, Redis, and MongoDB — with first-class Axum integration, HTTP request middleware, and the default process collector.

## Crates

```
┌──────────────┐   ┌──────────────┐   ┌──────────────┐
│ metrics-     │   │ metrics-     │   │ metrics-     │
│ postgres     │   │ redis        │   │ mongo        │
└──────┬───────┘   └──────┬───────┘   └──────┬───────┘
       │                  │                  │
       └──────────────────┼──────────────────┘
                          │
                  ┌───────┴────────┐
                  │  metrics-core  │
                  │  (trait, pool, │
                  │   server, cfg) │
                  └───────┬────────┘
                          │
                  ┌───────┴────────┐       ┌──────────────────┐
                  │   prometheus   │◄──────│   metrics-axum   │
                  │   (registry)   │       │  (layer, handler, │
                  └────────────────┘       │   state, builder) │
                                           └──────────────────┘
```

### `metrics-core`
Shared foundation: `MetricsCollector` trait, `PoolMetrics`, `CollectorConfig`, error types, and pre-tuned histogram buckets.

### `metrics-postgres`
Polls `pg_stat_database`, `pg_stat_user_tables`, `pg_stat_activity`, `pg_locks`, replication lag, database size. App-level `query_duration_seconds` and `rows_returned` histograms.

### `metrics-redis`
Parses `INFO ALL`: clients, memory, keyspace per-db, command stats, persistence, replication, CPU, hit/miss rates. App-level `command_duration_seconds` histogram.

### `metrics-mongo`
Runs `serverStatus`: connections, opcounters, network, memory, WiredTiger cache, cursors, documents, global lock, asserts, replication. App-level `op_duration_seconds` histogram.

### `metrics-axum` ← NEW
Full Axum integration:

| Component | What it does |
|---|---|
| `PrometheusMetrics` | Shared state (builder pattern) owning the registry, HTTP metrics, and collector handles |
| `PrometheusMetrics::builder("ns").with_process_collector().build()` | Registers the default `process_*` metrics (CPU, memory, FDs, threads, start time) |
| `HttpMetricsLayer` | Tower middleware recording `http_requests_total{method,path,status}`, `http_request_duration_seconds{method,path}`, `http_requests_in_flight`, `http_response_size_bytes{method,path}` |
| `metrics_handler` | `GET /metrics` handler encoding the full registry in Prometheus text format |
| `health_handler` | `GET /health` liveness check returning collector count |
| `prom.add_collector(c, interval)` | Register + start any `MetricsCollector` in one call |
| `prom.shutdown()` | Abort all background tasks (also runs on `Drop`) |

## Quick Start (Axum)

```rust
use std::sync::Arc;
use axum::{routing::get, Router};
use metrics_axum::{metrics_handler, health_handler, PrometheusMetrics};
use metrics_core::collector::CollectorConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Build shared Prometheus state with process metrics
    let prom = Arc::new(
        PrometheusMetrics::builder("myapp")
            .with_process_collector()
            .build()?,
    );

    let config = CollectorConfig {
        namespace: "myapp".into(),
        ..Default::default()
    };

    // 2. Register database collectors
    // -- PostgreSQL
    let pg_pool = /* your deadpool-postgres pool */;
    let pg = metrics_postgres::PostgresMetrics::new(pg_pool, config.clone())?;
    prom.add_collector(pg, config.collect_interval)?;

    // -- Redis
    let redis_client = redis::Client::open("redis://127.0.0.1/")?;
    let rd = metrics_redis::RedisMetrics::new(redis_client, config.clone())?;
    prom.add_collector(rd, config.collect_interval)?;

    // -- MongoDB
    let mongo_client = mongodb::Client::with_uri_str("mongodb://localhost:27017").await?;
    let mg = metrics_mongo::MongoMetrics::new(mongo_client, "admin", config.clone())?;
    prom.add_collector(mg, config.collect_interval)?;

    // 3. Build the Axum router
    let app = Router::new()
        // Observability routes (outside the metrics layer to avoid recursion)
        .route("/metrics", get(metrics_handler).with_state(prom.clone()))
        .route("/health",  get(health_handler).with_state(prom.clone()))
        // Application routes — wrapped with the HTTP metrics middleware
        .nest("/api", api_routes()
            .layer(prom.http_layer())  // ← records request_total, duration, in_flight
        );

    // 4. Serve
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async { tokio::signal::ctrl_c().await.ok(); prom.shutdown(); })
        .await?;

    Ok(())
}
```

## What `/metrics` Returns

A single scrape of `http://localhost:3000/metrics` produces:

```text
# Process metrics (from prometheus::process_collector)
process_cpu_seconds_total 0.42
process_open_fds 23
process_resident_memory_bytes 4.2e+07
process_start_time_seconds 1.70e+09
process_threads 8
process_virtual_memory_bytes 1.2e+09

# HTTP request metrics (from HttpMetricsLayer)
myapp_http_requests_total{method="GET",path="/api/users",status="200"} 147
myapp_http_request_duration_seconds_bucket{method="GET",path="/api/users",le="0.01"} 140
myapp_http_requests_in_flight 3
myapp_http_response_size_bytes_bucket{method="GET",path="/api/users",le="10000"} 147

# PostgreSQL metrics (from metrics-postgres collector)
myapp_postgres_pool_active_connections 4
myapp_postgres_pool_idle_connections 6
myapp_postgres_pool_acquire_duration_seconds_bucket{le="0.001"} 850
myapp_postgres_db_size_bytes 1.5e+08
myapp_postgres_backends{state="active"} 4
myapp_postgres_table_seq_scan{schema="public",table="users"} 12400
myapp_postgres_replication_lag_seconds 0.003
myapp_postgres_lock_count{mode="AccessShareLock"} 2
myapp_postgres_query_duration_seconds_bucket{query="list_users",le="0.01"} 990

# Redis metrics (from metrics-redis collector)
myapp_redis_connected_clients 12
myapp_redis_used_memory_bytes 5.2e+06
myapp_redis_keyspace_hit_rate 0.97
myapp_redis_db_keys{db="db0"} 15420
myapp_redis_command_duration_seconds_bucket{cmd="GET",le="0.001"} 9800

# MongoDB metrics (from metrics-mongo collector)
myapp_mongo_connections_current 8
myapp_mongo_opcounters_total{op="query"} 45000
myapp_mongo_wiredtiger_cache_bytes 1.2e+08
myapp_mongo_op_duration_seconds_bucket{op="find_orders",le="0.025"} 880
```

## Instrumenting Application Code

Each exporter exposes methods to record application-level latencies:

```rust
// PostgreSQL — record query time and row count
pg.observe_query("get_user_by_id", duration.as_secs_f64(), row_count as f64);

// Redis — record command time
rd.observe_command("GET", duration.as_secs_f64());

// MongoDB — record operation time
mg.observe_op("find_orders", duration.as_secs_f64());
```

The HTTP layer automatically instruments all requests passing through it — no per-handler code needed.

## Configuration

`CollectorConfig` fields:

| Field | Default | Description |
|---|---|---|
| `collect_interval` | 15s | Backend polling frequency |
| `namespace` | `""` | Prometheus namespace prefix |
| `const_labels` | `{}` | Labels on every metric |

## Full Metric Reference

See each crate's module-level docs for the complete list. Key categories:

| Category | Postgres | Redis | Mongo | HTTP |
|---|---|---|---|---|
| Connections / pool | ✓ | ✓ | ✓ | in_flight |
| Query / command latency | histogram | histogram | histogram | histogram |
| Throughput counters | tup_*, xact_* | command_calls | opcounters | requests_total |
| Memory | — | used_memory, RSS, frag | resident, virtual, WT cache | — |
| Replication | lag_seconds | offset, slaves | is_master | — |
| Cache / hit rate | blks_hit/read | hit_rate, evictions | WT cache | — |
| Locks / queues | lock_count | blocked_clients | globalLock queues | — |
| Process | pid, cpu, fds, mem, threads | (via process collector) | | |

## License

MIT
