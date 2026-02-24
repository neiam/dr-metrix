# dr-metrix-core

Core traits and types for the **dr-metrix** Prometheus metrics library.

This crate provides the shared foundation used by all dr-metrix collector crates:

- `MetricsCollector` — async trait implemented by every collector (postgres, redis, mongo, …)
- `CollectorConfig` — shared configuration (namespace, collection interval, const labels)
- `MetricsError` — unified error type
- `DEFAULT_QUERY_BUCKETS` — sensible histogram buckets for database query latencies

## Installation

```toml
[dependencies]
dr-metrix-core = "0.1"
```

## Usage

Implement `MetricsCollector` to build your own collector:

```rust
use std::sync::Arc;
use dr_metrix_core::{CollectorConfig, MetricsCollector, Result};

struct MyMetrics { /* prometheus gauges, histograms … */ }

impl MetricsCollector for MyMetrics {
    fn name(&self) -> &'static str { "my_metrics" }

    fn register(&self, registry: &prometheus::Registry) -> Result<()> {
        // registry.register(Box::new(self.my_gauge.clone()))?;
        Ok(())
    }

    async fn collect(&self) -> Result<()> {
        // scrape your data source and update gauges
        Ok(())
    }
}
```

The default `spawn_collect_loop` implementation launches a background tokio task that calls `collect()` on the configured interval using `MissedTickBehavior::Delay`.

```rust
let collector = Arc::new(MyMetrics { /* … */ });
let handle = collector.clone().spawn_collect_loop(config.collect_interval);
```

## Configuration

```rust
use std::time::Duration;
use dr_metrix_core::CollectorConfig;

let config = CollectorConfig {
    namespace: "myapp".into(),
    collect_interval: Duration::from_secs(30),
    const_labels: [("env".into(), "prod".into())].into(),
};
```
