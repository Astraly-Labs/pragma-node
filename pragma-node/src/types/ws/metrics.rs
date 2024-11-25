use prometheus::{CounterVec, Opts};
use strum::Display;

use crate::metrics::MetricsRegistry;

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

pub struct WsMetrics {
    interactions: CounterVec,
}

impl WsMetrics {
    pub fn new(registry: &MetricsRegistry) -> Result<Self, prometheus::Error> {
        let interactions = registry.register(CounterVec::new(
            Opts::new("ws_interactions", "Number of WebSocket interactions"),
            &["interaction", "status"],
        )?)?;

        Ok(Self { interactions })
    }

    pub fn record_interaction(&self, interaction: Interaction, status: Status) {
        self.interactions
            .with_label_values(&[
                interaction.to_string().as_str(),
                status.to_string().as_str(),
            ])
            .inc();
    }
}
