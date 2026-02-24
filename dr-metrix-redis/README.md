# dr-metrix-redis

Redis Prometheus metrics collector for **dr-metrix**, backed by [fred](https://crates.io/crates/fred) v10.

Parses `INFO ALL` on a configurable interval to expose client, memory, stats, keyspace, and replication metrics.

## Installation

```toml
[dependencies]
dr-metrix-redis = "0.1"
fred = "10"
```

## Usage

```rust
use fred::prelude::*;
use dr_metrix_core::collector::CollectorConfig;
use dr_metrix_redis::RedisMetrics;

// Build a fred connection pool
let cfg = Config::from_url("redis://127.0.0.1:6379")?;
let pool = Builder::from_config(cfg).build_pool(5)?;
let _conn = pool.connect();
pool.wait_for_connect().await?;

let config = CollectorConfig { namespace: "myapp".into(), ..Default::default() };
let redis = RedisMetrics::new(pool.clone(), config.clone())?;

// register with PrometheusMetrics (dr-metrix-axum)
prom.add_collector(redis, config.collect_interval)?;
```

### App-level command timing

```rust
let start = std::time::Instant::now();
// … run your redis command …
redis_metrics.observe_command("get", start.elapsed().as_secs_f64());
```

## Metrics exposed

| Metric | Type | Description |
|--------|------|-------------|
| `{ns}_redis_pool_connections` | IntGauge | Pool size |
| `{ns}_redis_connected_clients` | IntGauge | Connected clients |
| `{ns}_redis_blocked_clients` | IntGauge | Blocked clients |
| `{ns}_redis_used_memory_bytes` | IntGauge | Used memory |
| `{ns}_redis_used_memory_rss` | IntGauge | RSS memory |
| `{ns}_redis_mem_fragmentation_ratio` | Gauge | Fragmentation ratio |
| `{ns}_redis_keyspace_hits` | IntGauge | Keyspace hits |
| `{ns}_redis_keyspace_misses` | IntGauge | Keyspace misses |
| `{ns}_redis_evicted_keys` | IntGauge | Evicted keys |
| `{ns}_redis_total_commands_processed` | IntGauge | Total commands processed |
| `{ns}_redis_db_keys` | IntGaugeVec (`db`) | Keys per database |
| `{ns}_redis_db_expires` | IntGaugeVec (`db`) | Expiring keys per database |
| `{ns}_redis_replication_offset` | IntGauge | Replication offset |
| `{ns}_redis_connected_slaves` | IntGauge | Connected replicas |
| `{ns}_redis_command_duration_seconds` | HistogramVec (`cmd`) | App command latency |
