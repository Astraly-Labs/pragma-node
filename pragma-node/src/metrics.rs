use prometheus::{Error as PrometheusError, Registry};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct MetricsRegistry(Arc<Registry>);

impl MetricsRegistry {
    pub fn new() -> Self {
        MetricsRegistry(Arc::new(Registry::new()))
    }

    pub fn register<T: Clone + prometheus::core::Collector + 'static>(
        &self,
        metric: T,
    ) -> Result<T, PrometheusError> {
        self.0.register(Box::new(metric.clone()))?;
        Ok(metric)
    }

    pub fn registry(&self) -> Arc<Registry> {
        self.0.clone()
    }
}
