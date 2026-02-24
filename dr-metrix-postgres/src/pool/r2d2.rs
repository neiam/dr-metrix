use async_trait::async_trait;
use diesel::deserialize::QueryableByName;
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::row::NamedRow;
use diesel::sql_types::Text;
use diesel::RunQueryDsl;
use dr_metrix_core::error::{MetricsError, Result};
use serde_json::{Map, Value};

use super::{PoolAdapter, PoolStatus};

struct JsonRow {
    row_json: String,
}

impl<DB> QueryableByName<DB> for JsonRow
where
    DB: diesel::backend::Backend,
    String: diesel::deserialize::FromSql<Text, DB>,
{
    fn build<'a>(
        row: &impl NamedRow<'a, DB>,
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let row_json = NamedRow::get::<Text, String>(row, "row_json")?;
        Ok(Self { row_json })
    }
}

#[async_trait]
impl PoolAdapter for Pool<ConnectionManager<PgConnection>> {
    async fn query_json(&self, sql: &str) -> Result<Vec<Map<String, Value>>> {
        let pool = self.clone();
        let wrapped = format!("SELECT row_to_json(t)::text AS row_json FROM ({sql}) t");

        let rows: Vec<JsonRow> = tokio::task::spawn_blocking(move || {
            let mut conn = pool.get().map_err(MetricsError::database)?;
            diesel::sql_query(&wrapped)
                .load::<JsonRow>(&mut *conn)
                .map_err(MetricsError::database)
        })
        .await
        .map_err(MetricsError::database)??;

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let val: Value =
                serde_json::from_str(&row.row_json).map_err(MetricsError::database)?;
            if let Value::Object(map) = val {
                result.push(map);
            }
        }
        Ok(result)
    }

    fn pool_status(&self) -> PoolStatus {
        let state = self.state();
        PoolStatus {
            max_size: self.max_size(),
            available: state.idle_connections,
            in_use: state.connections.saturating_sub(state.idle_connections),
        }
    }
}
