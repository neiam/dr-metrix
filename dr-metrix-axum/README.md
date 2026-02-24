# dr-metrix-axum

Axum middleware and `/metrics` handler for exposing Prometheus metrics.

Provides:
- `PrometheusMetrics` — holds the registry, HTTP layer, and optional background collectors
- `metrics_handler` — Axum handler that renders the Prometheus text exposition format
- `HttpMetricsLayer` — Tower middleware that records `request_duration_seconds` labeled by `method`, `path`, and `status`

## Installation

```toml
[dependencies]
dr-metrix-axum = "0.1"
prometheus = { version = "0.13", features = ["process"] }
```

## Usage

```rust
use axum::{routing::get, Router};
use dr_metrix_axum::{metrics_handler, PrometheusMetrics};

#[tokio::main]
async fn main() {
    let prom = PrometheusMetrics::builder("myapp")
        .with_process_collector()   // exposes process_* metrics
        .build()
        .expect("failed to build prometheus metrics");

    // Wrap your API routes with the HTTP timing layer
    let api = Router::new()
        .route("/hello", get(|| async { "hello" }))
        .layer(prom.http_layer());

    let app = Router::new()
        .route("/metrics", get(metrics_handler).with_state(prom.clone()))
        .merge(api);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

### Adding collectors

Use `PrometheusMetrics::add_collector` to register a `MetricsCollector` and start its background scrape loop:

```rust
use dr_metrix_core::collector::CollectorConfig;
use dr_metrix_postgres::PostgresMetrics;

let pg_config = CollectorConfig { namespace: "myapp".into(), ..Default::default() };
let pg_metrics = PostgresMetrics::new(pool, pg_config.clone())?;
prom.add_collector(pg_metrics, pg_config.collect_interval)?;
```

### HTTP metrics

Every request through `.layer(prom.http_layer())` records an observation in the `{namespace}_request_duration_seconds` histogram with labels:

| Label    | Example        |
|----------|----------------|
| `method` | `GET`          |
| `path`   | `/api/users`   |
| `status` | `200`          |
