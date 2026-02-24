# dr-metrix-postgres

PostgreSQL Prometheus metrics collector for **dr-metrix**.

Collects pool stats, `pg_stat_database`, `pg_stat_user_tables`, and `pg_locks` on a configurable interval. Supports multiple connection pool backends via feature flags.

## Installation

```toml
# deadpool-diesel (default)
[dependencies]
dr-metrix-postgres = "0.1"

# diesel r2d2
[dependencies]
dr-metrix-postgres = { version = "0.1", default-features = false, features = ["r2d2"] }

# sea-orm
[dependencies]
dr-metrix-postgres = { version = "0.1", default-features = false, features = ["sea-orm"] }
```

## Features

| Feature        | Pool type                                     | Default |
|----------------|-----------------------------------------------|---------|
| `diesel-async` | `deadpool_diesel::postgres::Pool`             | yes     |
| `r2d2`         | `diesel::r2d2::Pool<ConnectionManager<PgConnection>>` | no |
| `sea-orm`      | `sea_orm::DatabaseConnection`                 | no      |

## Usage

```rust
use dr_metrix_core::collector::CollectorConfig;
use dr_metrix_postgres::PostgresMetrics;

// pool: deadpool_diesel::postgres::Pool  (or r2d2::Pool, or DatabaseConnection)
let config = CollectorConfig { namespace: "myapp".into(), ..Default::default() };
let pg = PostgresMetrics::new(pool, config.clone())?;

// register with PrometheusMetrics (dr-metrix-axum)
prom.add_collector(pg, config.collect_interval)?;
```

### App-level query timing

```rust
let start = std::time::Instant::now();
// … run your query …
pg.observe_query("fetch_users", start.elapsed().as_secs_f64(), row_count as f64);
```

## Metrics exposed

| Metric | Type | Description |
|--------|------|-------------|
| `{ns}_pool_active` | Gauge | Connections in use |
| `{ns}_pool_idle` | Gauge | Idle connections |
| `{ns}_pool_max` | Gauge | Max pool size |
| `{ns}_db_numbackends` | IntGaugeVec (`database`) | Active backends |
| `{ns}_db_xact_commit` | IntGaugeVec (`database`) | Committed transactions |
| `{ns}_db_xact_rollback` | IntGaugeVec (`database`) | Rolled-back transactions |
| `{ns}_db_blks_hit` | IntGaugeVec (`database`) | Buffer cache hits |
| `{ns}_db_blks_read` | IntGaugeVec (`database`) | Disk blocks read |
| `{ns}_db_size_bytes` | IntGaugeVec (`database`) | Database size |
| `{ns}_table_seq_scan` | IntGaugeVec (`schema,table`) | Sequential scans |
| `{ns}_table_idx_scan` | IntGaugeVec (`schema,table`) | Index scans |
| `{ns}_table_live_tup` | IntGaugeVec (`schema,table`) | Live tuples |
| `{ns}_table_dead_tup` | IntGaugeVec (`schema,table`) | Dead tuples |
| `{ns}_lock_count` | IntGaugeVec (`mode`) | Granted locks by mode |
| `{ns}_query_duration_seconds` | HistogramVec (`query`) | App query latency |
| `{ns}_rows_returned` | HistogramVec (`query`) | Rows returned per query |
