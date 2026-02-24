use async_trait::async_trait;
use dr_metrix_core::error::{MetricsError, Result};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use serde_json::{Map, Value};

use super::{PoolAdapter, PoolStatus};

#[async_trait]
impl PoolAdapter for DatabaseConnection {
    async fn query_json(&self, sql: &str) -> Result<Vec<Map<String, Value>>> {
        let wrapped = format!("SELECT row_to_json(t)::text AS row_json FROM ({sql}) t");
        let stmt = Statement::from_string(DatabaseBackend::Postgres, wrapped);
        let rows = self
            .query_all(stmt)
            .await
            .map_err(|e| MetricsError::database(e))?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let json_str: String = row
                .try_get("", "row_json")
                .map_err(|e| MetricsError::database(e))?;
            let val: Value =
                serde_json::from_str(&json_str).map_err(|e| MetricsError::database(e))?;
            if let Value::Object(map) = val {
                out.push(map);
            }
        }
        Ok(out)
    }

    fn pool_status(&self) -> PoolStatus {
        // sea-orm doesn't expose pool stats uniformly across all backends.
        // Return a best-effort value — callers should treat 0 as "unknown".
        PoolStatus {
            max_size: 0,
            available: 0,
            in_use: 0,
        }
    }
}
