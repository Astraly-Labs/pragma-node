use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use pragma_common::types::DataType;
use pragma_entities::EntryError;
use utoipa::{ToResponse, ToSchema};

use crate::infra::repositories::entry_repository::MedianEntryWithComponents;
use crate::types::pricer::{IndexPricer, Pricer};
use crate::types::timestamp::UnixTimestamp;
use crate::types::ws::{ChannelHandler, Subscriber, SubscriptionType};
use crate::utils::only_existing_pairs;
use crate::AppState;

#[derive(Debug, Default, Serialize, Deserialize, ToResponse, ToSchema)]
pub struct AssetOraclePrice {
    num_sources_aggregated: usize,
    pair_id: String,
    price: String,
}

#[derive(Debug, Default, Serialize, Deserialize, ToResponse, ToSchema)]
pub struct SubscribeToPriceResponse {
    pub oracle_prices: Vec<AssetOraclePrice>,
    #[schema(value_type = i64)]
    pub timestamp: UnixTimestamp,
}

#[utoipa::path(
    get,
    path = "/node/v1/data/price/subscribe",
    responses(
        (
            status = 200,
            description = "Subscribe to a list of pairs' prices",
            body = [SubscribeToPriceResponse]
        )
    )
)]
pub async fn subscribe_to_price(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    ConnectInfo(client_addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| create_new_subscriber(socket, state, client_addr))
}

/// Interval in milliseconds that the channel will update the client with the latest prices.
const CHANNEL_UPDATE_INTERVAL_IN_MS: u64 = 500;

async fn create_new_subscriber(socket: WebSocket, app_state: AppState, client_addr: SocketAddr) {
    let (mut subscriber, _) = match Subscriber::<SubscriptionState>::new(
        socket,
        client_addr.ip(),
        Arc::new(app_state),
        None,
        CHANNEL_UPDATE_INTERVAL_IN_MS,
    )
    .await
    {
        Ok(subscriber) => subscriber,
        Err(e) => {
            tracing::error!("Failed to register subscriber: {}", e);
            return;
        }
    };

    // Main event loop for the subscriber
    let handler = WsEntriesHandler;
    let status = subscriber.listen(handler).await;
    if let Err(e) = status {
        tracing::error!(
            "[{}] Error occurred while listening to the subscriber: {:?}",
            subscriber.id,
            e
        );
    }
}

struct WsEntriesHandler;

impl ChannelHandler<SubscriptionState, SubscriptionRequest, EntryError> for WsEntriesHandler {
    async fn handle_client_msg(
        &mut self,
        subscriber: &mut Subscriber<SubscriptionState>,
        request: SubscriptionRequest,
    ) -> Result<(), EntryError> {
        let (existing_spot_pairs, _existing_perp_pairs) =
            only_existing_pairs(&subscriber.app_state.offchain_pool, request.pairs).await;
        let mut state = subscriber.state.lock().await;
        match request.msg_type {
            SubscriptionType::Subscribe => {
                state.add_spot_pairs(existing_spot_pairs);
            }
            SubscriptionType::Unsubscribe => {
                state.remove_spot_pairs(&existing_spot_pairs);
            }
        };
        let subscribed_pairs = state.get_subscribed_spot_pairs();
        drop(state);
        // We send an ack message to the client with the subscribed pairs (so
        // the client knows which pairs are successfully subscribed).
        if let Ok(ack_message) = serde_json::to_string(&SubscriptionAck {
            msg_type: request.msg_type,
            pairs: subscribed_pairs,
        }) {
            if subscriber.send_msg(ack_message).await.is_err() {
                let error_msg = "Message received but could not send ack message.";
                subscriber.send_err(error_msg).await;
            }
        } else {
            let error_msg = "Could not serialize ack message.";
            subscriber.send_err(error_msg).await;
        }
        Ok(())
    }

    async fn periodic_interval(
        &mut self,
        subscriber: &mut Subscriber<SubscriptionState>,
    ) -> Result<(), EntryError> {
        let subscription = subscriber.state.lock().await;
        if subscription.is_empty() {
            return Ok(());
        }
        let response = match self
            .get_subscribed_pairs_medians(&subscriber.app_state, &subscription)
            .await
        {
            Ok(response) => response,
            Err(e) => {
                drop(subscription);
                subscriber.send_err(&e.to_string()).await;
                return Err(e);
            }
        };
        drop(subscription);
        if let Ok(json_response) = serde_json::to_string(&response) {
            if subscriber.send_msg(json_response).await.is_err() {
                subscriber.send_err("Could not send prices.").await;
            }
        } else {
            subscriber.send_err("Could not serialize prices.").await;
        }
        Ok(())
    }
}

impl WsEntriesHandler {
    /// Get the current median entries for the subscribed pairs and sign them as Pragma.
    async fn get_subscribed_pairs_medians(
        &self,
        state: &AppState,
        subscription: &SubscriptionState,
    ) -> Result<SubscribeToPriceResponse, EntryError> {
        let median_entries = self.get_all_entries(state, subscription).await?;

        let now = chrono::Utc::now().timestamp();

        let oracle_prices = median_entries
            .into_iter()
            .map(|entry| AssetOraclePrice {
                num_sources_aggregated: entry.components.len(),
                pair_id: entry.pair_id,
                price: entry.median_price.to_string(),
            })
            .collect();

        Ok(SubscribeToPriceResponse {
            timestamp: now,
            oracle_prices,
        })
    }

    /// Get index & mark prices for the subscribed pairs.
    async fn get_all_entries(
        &self,
        state: &AppState,
        subscription: &SubscriptionState,
    ) -> Result<Vec<MedianEntryWithComponents>, EntryError> {
        let index_pricer = IndexPricer::new(
            subscription.get_subscribed_spot_pairs(),
            DataType::SpotEntry,
        );

        let median_entries = index_pricer.compute(&state.offchain_pool).await?;

        Ok(median_entries)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SubscriptionRequest {
    msg_type: SubscriptionType,
    pairs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SubscriptionAck {
    msg_type: SubscriptionType,
    pairs: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SubscriptionState {
    spot_pairs: HashSet<String>,
}

impl SubscriptionState {
    fn is_empty(&self) -> bool {
        self.spot_pairs.is_empty()
    }

    fn add_spot_pairs(&mut self, pairs: Vec<String>) {
        self.spot_pairs.extend(pairs);
    }

    fn remove_spot_pairs(&mut self, pairs: &[String]) {
        for pair in pairs {
            self.spot_pairs.remove(pair);
        }
    }

    /// Get the subscribed spot pairs.
    fn get_subscribed_spot_pairs(&self) -> Vec<String> {
        self.spot_pairs.iter().cloned().collect()
    }
}
