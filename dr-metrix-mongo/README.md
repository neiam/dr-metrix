# dr-metrix-mongo

MongoDB Prometheus metrics collector for **dr-metrix**, using the official [mongodb](https://crates.io/crates/mongodb) v3 driver.

Runs `serverStatus` on a configurable interval to expose connection, opcode, memory, and WiredTiger cache metrics.

## Installation

```toml
[dependencies]
dr-metrix-mongo = "0.1"
mongodb = "3"
```

## Usage

```rust
use mongodb::Client;
use dr_metrix_core::collector::CollectorConfig;
use dr_metrix_mongo::MongoMetrics;

let client = Client::with_uri_str("mongodb://localhost:27017").await?;

let config = CollectorConfig { namespace: "myapp".into(), ..Default::default() };
let mongo = MongoMetrics::new(client, "admin", config.clone())?;

// register with PrometheusMetrics (dr-metrix-axum)
prom.add_collector(mongo, config.collect_interval)?;
```

### App-level operation timing

```rust
let start = std::time::Instant::now();
// … run your mongodb operation …
mongo_metrics.observe_op("find", start.elapsed().as_secs_f64());
```

## Metrics exposed

| Metric | Type | Description |
|--------|------|-------------|
| `{ns}_mongo_connections_current` | IntGauge | Current connections |
| `{ns}_mongo_connections_available` | IntGauge | Available connections |
| `{ns}_mongo_connections_total` | IntGauge | Total connections created |
| `{ns}_mongo_opcounters` | IntGaugeVec (`op`) | Op counts (insert/query/update/delete/getmore/command) |
| `{ns}_mongo_network_bytes_in` | IntGauge | Network bytes in |
| `{ns}_mongo_network_bytes_out` | IntGauge | Network bytes out |
| `{ns}_mongo_network_requests` | IntGauge | Network requests |
| `{ns}_mongo_resident_memory_bytes` | IntGauge | Resident memory (bytes) |
| `{ns}_mongo_virtual_memory_bytes` | IntGauge | Virtual memory (bytes) |
| `{ns}_mongo_wt_cache_bytes_in_use` | IntGauge | WiredTiger cache bytes in use |
| `{ns}_mongo_wt_cache_bytes_max` | IntGauge | WiredTiger cache max bytes |
| `{ns}_mongo_wt_cache_dirty_bytes` | IntGauge | WiredTiger dirty cache bytes |
| `{ns}_mongo_op_duration_seconds` | HistogramVec (`op`) | App operation latency |

> **Note:** opcounters are exposed as `IntGaugeVec` (not counters) because `serverStatus` returns absolute snapshot values, not deltas.
