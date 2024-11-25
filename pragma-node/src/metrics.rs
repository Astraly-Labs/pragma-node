use std::sync::Arc;

use opentelemetry::{metrics::Counter, KeyValue};
use strum::Display;

#[derive(Debug)]
pub struct MetricsRegistry {
    /// TODO(akhercha): See which additional metrics we want here?
    pub ws_metrics: WsMetricsRegistry,
}

impl MetricsRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            ws_metrics: Arc::try_unwrap(WsMetricsRegistry::new())
                .unwrap_or_else(|arc| (*arc).clone()),
        })
    }
}

#[derive(Debug, Clone)]
pub struct WsMetricsRegistry {
    metrics: std::collections::HashMap<String, WsMetrics>,
}

impl WsMetricsRegistry {
    pub fn new() -> Arc<Self> {
        let mut metrics = std::collections::HashMap::new();

        let endpoints = [
            "subscribe_to_entry",
            "subscribe_to_price",
            "subscribe_to_ohlc",
        ];
        for endpoint in endpoints.iter() {
            metrics.insert(endpoint.to_string(), WsMetrics::new(endpoint));
        }

        Arc::new(Self { metrics })
    }

    pub fn record_ws_interaction(
        &self,
        endpoint_name: &str,
        interaction: Interaction,
        status: Status,
    ) {
        if let Some(metrics) = self.metrics.get(endpoint_name) {
            metrics.record_interaction(interaction, status);
        } else {
            tracing::warn!("No metrics registered for WS endpoint: {}", endpoint_name);
        }
    }
}

#[derive(Display, Clone, Debug)]
pub enum Interaction {
    NewConnection,
    CloseConnection,
    ClientMessageDecode,
    ClientMessageProcess,
    ChannelUpdate,
    RateLimit,
}

#[derive(Display, Clone, Debug)]
pub enum Status {
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct WsMetrics {
    interactions: Counter<u64>,
}

impl WsMetrics {
    fn new(endpoint_name: &str) -> Self {
        let meter = opentelemetry::global::meter("pragma-node-meter");
        let interactions = meter
            .u64_counter(format!("{}_ws_interactions_total", endpoint_name))
            .with_description(format!(
                "Number of WebSocket interactions for {}",
                endpoint_name
            ))
            .with_unit("count")
            .init();

        Self { interactions }
    }

    fn record_interaction(&self, interaction: Interaction, status: Status) {
        self.interactions.add(
            1,
            &[
                KeyValue::new("interaction", interaction.to_string()),
                KeyValue::new("status", status.to_string()),
            ],
        );
    }
}
