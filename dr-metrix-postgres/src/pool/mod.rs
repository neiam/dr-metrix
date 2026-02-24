use async_trait::async_trait;
use dr_metrix_core::error::Result;

pub struct PoolStatus {
    pub max_size: u32,
    pub available: u32,
    pub in_use: u32,
}

#[async_trait]
pub trait PoolAdapter: Send + Sync + 'static {
    async fn query_json(
        &self,
        sql: &str,
    ) -> Result<Vec<serde_json::Map<String, serde_json::Value>>>;
    fn pool_status(&self) -> PoolStatus;
}

#[cfg(feature = "diesel-async")]
pub mod deadpool;

#[cfg(feature = "r2d2")]
pub mod r2d2;

#[cfg(feature = "sea-orm")]
pub mod seaorm;
