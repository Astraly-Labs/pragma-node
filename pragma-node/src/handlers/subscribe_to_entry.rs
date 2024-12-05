use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use pragma_common::types::DataType;
use pragma_entities::EntryError;
use utoipa::{ToResponse, ToSchema};

use crate::constants::starkex_ws::PRAGMA_ORACLE_NAME_FOR_STARKEX;
use crate::infra::repositories::entry_repository::MedianEntryWithComponents;
use crate::types::pricer::{IndexPricer, MarkPricer, Pricer};
use crate::types::timestamp::UnixTimestamp;
use crate::types::ws::{ChannelHandler, Subscriber, SubscriptionType};
use crate::utils::{only_existing_pairs, sign_data, StarkexPrice};
use crate::AppState;

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct SignedPublisherPrice {
    pub oracle_asset_id: String,
    pub oracle_price: String,
    pub signing_key: String,
    pub signature: String,
    pub timestamp: String,
}

#[derive(Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct AssetOraclePrice {
    pub global_asset_id: String,
    pub median_price: String,
    pub signature: String,
    pub signed_prices: Vec<SignedPublisherPrice>,
}

#[derive(Debug, Default, Serialize, Deserialize, ToResponse, ToSchema)]
pub struct SubscribeToEntryResponse {
    pub oracle_prices: Vec<AssetOraclePrice>,
    #[schema(value_type = i64)]
    pub timestamp: UnixTimestamp,
}

#[tracing::instrument(skip(state, ws), fields(endpoint_name = "subscribe_to_entry"))]
pub async fn subscribe_to_entry(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    ConnectInfo(client_addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    if state.pragma_signer.is_none() {
        tracing::error!("No Pragma signer found");
        return (StatusCode::LOCKED, "Locked: Pragma signer not found").into_response();
    }
    ws.on_upgrade(move |socket| create_new_subscriber(socket, state, client_addr))
}

/// Interval in milliseconds that the channel will update the client with the latest prices.
const CHANNEL_UPDATE_INTERVAL_IN_MS: u64 = 500;

#[tracing::instrument(
    skip(socket, app_state),
    fields(
        subscriber_id,
        client_ip = %client_addr.ip()
    )
)]
async fn create_new_subscriber(socket: WebSocket, app_state: AppState, client_addr: SocketAddr) {
    let (mut subscriber, _) = match Subscriber::<SubscriptionState>::new(
        "subscribe_to_entry".into(),
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
    #[tracing::instrument(
        skip(self, subscriber),
        fields(
            subscriber_id = %subscriber.id,
            msg_type = ?request.msg_type,
            pairs = ?request.pairs
        )
    )]
    async fn handle_client_msg(
        &mut self,
        subscriber: &mut Subscriber<SubscriptionState>,
        request: SubscriptionRequest,
    ) -> Result<(), EntryError> {
        let (existing_spot_pairs, existing_perp_pairs) =
            only_existing_pairs(&subscriber.app_state.offchain_pool, request.pairs).await;
        let mut state = subscriber.state.lock().await;
        match request.msg_type {
            SubscriptionType::Subscribe => {
                state.add_spot_pairs(existing_spot_pairs);
                state.add_perp_pairs(existing_perp_pairs);
            }
            SubscriptionType::Unsubscribe => {
                state.remove_spot_pairs(&existing_spot_pairs);
                state.remove_perp_pairs(&existing_perp_pairs);
            }
        };
        let subscribed_pairs = state.get_fmt_subscribed_pairs();
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

    #[tracing::instrument(
        skip(self, subscriber),
        fields(
            subscriber_id = %subscriber.id
        )
    )]
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
    #[tracing::instrument(
        skip(self, state, subscription),
        fields(
            spot_pairs = ?subscription.get_subscribed_spot_pairs().len(),
            perp_pairs = ?subscription.get_subscribed_perp_pairs().len()
        )
    )]
    async fn get_subscribed_pairs_medians(
        &self,
        state: &AppState,
        subscription: &SubscriptionState,
    ) -> Result<SubscribeToEntryResponse, EntryError> {
        let median_entries = self.get_all_entries(state, subscription).await?;

        let mut response: SubscribeToEntryResponse = Default::default();
        let now = chrono::Utc::now().timestamp();

        let pragma_signer = state
            .pragma_signer
            .as_ref()
            // Should not happen, as the endpoint is disabled if the signer is not found.
            .ok_or(EntryError::InternalServerError)?;

        for entry in median_entries {
            let median_price = entry.median_price.clone();
            let pair_id = entry.pair_id.clone();
            let mut oracle_price: AssetOraclePrice = entry
                .try_into()
                .map_err(|_| EntryError::InternalServerError)?;

            let starkex_price = StarkexPrice {
                oracle_name: PRAGMA_ORACLE_NAME_FOR_STARKEX.to_string(),
                pair_id: pair_id.clone(),
                timestamp: now as u64,
                price: median_price,
            };
            let signature =
                sign_data(pragma_signer, &starkex_price).map_err(|_| EntryError::InvalidSigner)?;

            oracle_price.signature = signature;
            response.oracle_prices.push(oracle_price);
        }
        response.timestamp = now;
        Ok(response)
    }

    /// Get index & mark prices for the subscribed pairs.
    #[tracing::instrument(skip(self, state, subscription))]
    async fn get_all_entries(
        &self,
        state: &AppState,
        subscription: &SubscriptionState,
    ) -> Result<Vec<MedianEntryWithComponents>, EntryError> {
        let index_pricer = IndexPricer::new(
            subscription.get_subscribed_spot_pairs(),
            DataType::SpotEntry,
        );

        let (usd_pairs, non_usd_pairs): (Vec<String>, Vec<String>) = subscription
            .get_subscribed_perp_pairs()
            .into_iter()
            .partition(|pair| {
                tracing::debug!("Checking pair for USD: {}", pair);
                pair.ends_with("USD")
            });
        tracing::debug!(
            "USD pairs: {:?}, non-USD pairs: {:?}",
            usd_pairs,
            non_usd_pairs
        );
        let mark_pricer_usd = MarkPricer::new(usd_pairs, DataType::PerpEntry);
        let mark_pricer_non_usd = MarkPricer::new(non_usd_pairs, DataType::PerpEntry);

        // Compute entries concurrently
        let (index_entries, usd_mark_entries, non_usd_mark_entries) = tokio::join!(
            index_pricer.compute(&state.offchain_pool),
            mark_pricer_usd.compute(&state.offchain_pool),
            mark_pricer_non_usd.compute(&state.offchain_pool)
        );

        let mut median_entries = vec![];
        median_entries.extend(index_entries.unwrap_or_default());
        median_entries.extend(usd_mark_entries.unwrap_or_default());
        median_entries.extend(non_usd_mark_entries.unwrap_or_default());
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
    perp_pairs: HashSet<String>,
}

impl SubscriptionState {
    fn is_empty(&self) -> bool {
        self.spot_pairs.is_empty() && self.perp_pairs.is_empty()
    }

    fn add_spot_pairs(&mut self, pairs: Vec<String>) {
        self.spot_pairs.extend(pairs);
    }

    fn add_perp_pairs(&mut self, pairs: Vec<String>) {
        self.perp_pairs.extend(pairs);
    }

    fn remove_spot_pairs(&mut self, pairs: &[String]) {
        for pair in pairs {
            self.spot_pairs.remove(pair);
        }
    }

    fn remove_perp_pairs(&mut self, pairs: &[String]) {
        for pair in pairs {
            self.perp_pairs.remove(pair);
        }
    }

    /// Get the subscribed spot pairs.
    fn get_subscribed_spot_pairs(&self) -> Vec<String> {
        self.spot_pairs.iter().cloned().collect()
    }

    /// Get the subscribed perps pairs (without suffix).
    fn get_subscribed_perp_pairs(&self) -> Vec<String> {
        self.perp_pairs.iter().cloned().collect()
    }

    /// Get the subscribed perps pairs with the MARK suffix.
    fn get_fmt_subscribed_perp_pairs(&self) -> Vec<String> {
        self.perp_pairs
            .iter()
            .map(|pair| format!("{}:MARK", pair))
            .collect()
    }

    /// Get all the currently subscribed pairs.
    /// (Spot and Perp pairs with the suffix)
    fn get_fmt_subscribed_pairs(&self) -> Vec<String> {
        let mut spot_pairs = self.get_subscribed_spot_pairs();
        let perp_pairs = self.get_fmt_subscribed_perp_pairs();
        spot_pairs.extend(perp_pairs);
        spot_pairs
    }
}
