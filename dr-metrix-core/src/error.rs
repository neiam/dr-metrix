pub type Result<T> = std::result::Result<T, MetricsError>;

#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    #[error("prometheus error: {0}")]
    Prometheus(#[from] prometheus::Error),
    #[error("database error: {0}")]
    Database(String),
    #[error("collector error: {0}")]
    Collector(String),
}

impl MetricsError {
    pub fn database(msg: impl std::fmt::Display) -> Self {
        Self::Database(msg.to_string())
    }
    pub fn collector(msg: impl std::fmt::Display) -> Self {
        Self::Collector(msg.to_string())
    }
}
