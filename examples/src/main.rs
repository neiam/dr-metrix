//! # Full Axum Integration Example
//!
//! Demonstrates wiring PostgreSQL, Redis, and MongoDB metrics exporters into
//! a real Axum application alongside:
//!
//! - The default Prometheus **process collector** (CPU, memory, FDs, threads)
//! - An HTTP **request metrics middleware** layer (requests_total, duration, in-flight)
//! - Application-level business route instrumentation
//! - Graceful shutdown
//!
//! ```text
//! PG_URL="host=localhost user=app dbname=mydb" \
//! REDIS_URL="redis://127.0.0.1:6379" \
//! MONGO_URL="mongodb://127.0.0.1:27017" \
//!   cargo run -p examples
//! ```
//!
//! Then:
//! - `curl http://localhost:3000/health`
//! - `curl http://localhost:3000/metrics`
//! - `curl http://localhost:3000/api/users`
//! - `curl -X POST http://localhost:3000/api/counter/hits/incr`
//! - `curl -X POST http://localhost:3000/api/counter/hits/decr`

use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use fred::clients::Pool as RedisPool;
use fred::interfaces::KeysInterface;
use dr_metrix_axum::{health_handler, metrics_handler, PrometheusMetrics};
use dr_metrix_core::collector::CollectorConfig;
use serde_json::{json, Value};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

/// Shared application state passed to all route handlers.
#[derive(Clone)]
#[allow(dead_code)]
struct AppState {
    prom: Arc<PrometheusMetrics>,
    pg_metrics: Option<Arc<dr_metrix_postgres::PostgresMetrics<deadpool_diesel::postgres::Pool>>>,
    redis_pool: Option<RedisPool>,
    redis_metrics: Option<Arc<dr_metrix_redis::RedisMetrics>>,
    mongo_metrics: Option<Arc<dr_metrix_mongo::MongoMetrics>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Logging ────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // ── Prometheus state (registry + process collector + HTTP metrics) ──
    let prom = Arc::new(
        PrometheusMetrics::builder("myapp")
            .with_process_collector()
            .build()?,
    );

    // ── Collector config shared by all backends ────────────────
    let config = CollectorConfig {
        namespace: "myapp".into(),
        ..Default::default()
    };

    // ── PostgreSQL (deadpool-diesel) ───────────────────────────
    let pg_metrics: Option<Arc<dr_metrix_postgres::PostgresMetrics<deadpool_diesel::postgres::Pool>>> =
        if let Ok(pg_url) = std::env::var("PG_URL") {
            let manager = deadpool_diesel::postgres::Manager::new(
                pg_url,
                deadpool_diesel::Runtime::Tokio1,
            );
            let pool = deadpool_diesel::postgres::Pool::builder(manager)
                .max_size(10)
                .build()?;

            let m = dr_metrix_postgres::PostgresMetrics::new(pool, config.clone())?;
            let arc = Arc::new(m);
            prom.add_collector((*arc).clone(), config.collect_interval)?;
            tracing::info!("PostgreSQL collector registered");
            Some(arc)
        } else {
            tracing::warn!("PG_URL not set — skipping PostgreSQL metrics");
            None
        };

    // ── Redis (fred v10 pool) ──────────────────────────────────
    let (redis_pool, redis_metrics): (Option<RedisPool>, Option<Arc<dr_metrix_redis::RedisMetrics>>) =
        if let Ok(redis_url) = std::env::var("REDIS_URL") {
            use fred::prelude::*;
            let cfg = Config::from_url(&redis_url)?;
            let pool = Builder::from_config(cfg).build_pool(5)?;
            let _conn = pool.connect();
            pool.wait_for_connect().await?;

            let m = dr_metrix_redis::RedisMetrics::new(pool.clone(), config.clone())?;
            let arc = Arc::new(m);
            prom.add_collector((*arc).clone(), config.collect_interval)?;
            tracing::info!("Redis collector registered");
            (Some(pool), Some(arc))
        } else {
            tracing::warn!("REDIS_URL not set — skipping Redis metrics");
            (None, None)
        };

    // ── MongoDB ────────────────────────────────────────────────
    let mongo_metrics: Option<Arc<dr_metrix_mongo::MongoMetrics>> =
        if let Ok(mongo_url) = std::env::var("MONGO_URL") {
            let client = mongodb::Client::with_uri_str(&mongo_url).await?;
            let m = dr_metrix_mongo::MongoMetrics::new(client, "admin", config.clone())?;
            let arc = Arc::new(m);
            prom.add_collector((*arc).clone(), config.collect_interval)?;
            tracing::info!("MongoDB collector registered");
            Some(arc)
        } else {
            tracing::warn!("MONGO_URL not set — skipping MongoDB metrics");
            None
        };

    // ── App state ──────────────────────────────────────────────
    let state = AppState {
        prom: prom.clone(),
        pg_metrics,
        redis_pool,
        redis_metrics,
        mongo_metrics,
    };

    // ── Router ─────────────────────────────────────────────────
    let app = Router::new()
        .route("/metrics", get(metrics_handler).with_state(prom.clone()))
        .route("/health", get(health_handler).with_state(prom.clone()))
        .nest(
            "/api",
            Router::new()
                .route("/users", get(list_users))
                .route("/orders", get(list_orders))
                .route("/counter/{key}/incr", post(counter_incr))
                .route("/counter/{key}/decr", post(counter_decr))
                .with_state(state.clone())
                .layer(prom.http_layer()),
        )
        .layer(TraceLayer::new_for_http());

    // ── Start server ───────────────────────────────────────────
    let addr = std::env::var("LISTEN").unwrap_or_else(|_| "0.0.0.0:3000".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(addr = %addr, "server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(prom))
        .await?;

    Ok(())
}

// ───────────────────────────── Route handlers ────────────────────

async fn list_users(State(state): State<AppState>) -> Json<Value> {
    let start = Instant::now();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let elapsed = start.elapsed().as_secs_f64();

    if let Some(m) = &state.pg_metrics {
        m.observe_query("list_users", elapsed, 2.0);
    }

    tracing::debug!(duration_ms = elapsed * 1000.0, "list_users");

    Json(json!({
        "users": [
            {"id": 1, "name": "Alice"},
            {"id": 2, "name": "Bob"},
        ]
    }))
}

async fn list_orders(State(_state): State<AppState>) -> Json<Value> {
    let start = Instant::now();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let elapsed = start.elapsed().as_secs_f64();

    tracing::debug!(duration_ms = elapsed * 1000.0, "list_orders");

    Json(json!({
        "orders": [
            {"id": 100, "total": 59.99},
        ]
    }))
}

// ───────────────────────────── Counter handlers ──────────────────

/// `POST /api/counter/:key/incr` — atomically increment a Redis counter.
///
/// Returns `{"key": "...", "value": N}` or 503 if Redis is unavailable.
async fn counter_incr(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    counter_op(&state, &key, "incr").await
}

/// `POST /api/counter/:key/decr` — atomically decrement a Redis counter.
///
/// Returns `{"key": "...", "value": N}` or 503 if Redis is unavailable.
async fn counter_decr(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    counter_op(&state, &key, "decr").await
}

async fn counter_op(state: &AppState, key: &str, op: &str) -> impl IntoResponse {
    let Some(pool) = &state.redis_pool else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "Redis not configured"})),
        );
    };

    let start = Instant::now();
    let result: Result<i64, _> = if op == "incr" {
        pool.incr(key).await
    } else {
        pool.decr(key).await
    };
    let elapsed = start.elapsed().as_secs_f64();

    if let Some(m) = &state.redis_metrics {
        m.observe_command(op, elapsed);
    }

    match result {
        Ok(value) => {
            tracing::debug!(key, op, value, "counter op");
            (StatusCode::OK, Json(json!({"key": key, "value": value})))
        }
        Err(e) => {
            tracing::error!(key, op, error = %e, "Redis command failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        }
    }
}

// ───────────────────────────── Graceful shutdown ─────────────────

async fn shutdown_signal(prom: Arc<PrometheusMetrics>) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received — stopping collectors");
    prom.shutdown();
}
