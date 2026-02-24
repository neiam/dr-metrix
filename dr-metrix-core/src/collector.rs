use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::error::Result;

pub struct CollectorConfig {
    pub namespace: String,
    pub collect_interval: Duration,
    pub const_labels: HashMap<String, String>,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            namespace: String::new(),
            collect_interval: Duration::from_secs(15),
            const_labels: HashMap::new(),
        }
    }
}

impl Clone for CollectorConfig {
    fn clone(&self) -> Self {
        Self {
            namespace: self.namespace.clone(),
            collect_interval: self.collect_interval,
            const_labels: self.const_labels.clone(),
        }
    }
}

#[async_trait]
pub trait MetricsCollector: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn register(&self, registry: &prometheus::Registry) -> Result<()>;
    async fn collect(&self) -> Result<()>;

    fn spawn_collect_loop(self: Arc<Self>, interval: Duration) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                if let Err(e) = self.collect().await {
                    tracing::warn!(
                        collector = self.name(),
                        error = %e,
                        "collector error"
                    );
                }
            }
        })
    }
}
