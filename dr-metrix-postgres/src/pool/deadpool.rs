use async_trait::async_trait;
use diesel::deserialize::QueryableByName;
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
impl PoolAdapter for deadpool_diesel::postgres::Pool {
    async fn query_json(&self, sql: &str) -> Result<Vec<Map<String, Value>>> {
        let wrapped = format!("SELECT row_to_json(t)::text AS row_json FROM ({sql}) t");
        let conn = self.get().await.map_err(MetricsError::database)?;

        let rows: Vec<JsonRow> = conn
            .interact(move |conn| diesel::sql_query(wrapped).load::<JsonRow>(conn))
            .await
            .map_err(MetricsError::database)?
            .map_err(MetricsError::database)?;

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
        let status = self.status();
        let in_use = status.size.saturating_sub(status.available) as u32;
        PoolStatus {
            max_size: status.max_size as u32,
            available: status.available as u32,
            in_use,
        }
    }
}
