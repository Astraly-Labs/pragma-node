use std::collections::HashSet;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use bigdecimal::BigDecimal;

use pragma_entities::EntryError;
use serde::{Deserialize, Serialize};
use serde_json::json;
use starknet::signers::SigningKey;
use tokio::time::interval;

use crate::handlers::entries::SubscribeToEntryResponse;
use crate::infra::repositories::entry_repository::get_current_median_entries_with_components;
use crate::utils::get_entry_hash;
use crate::AppState;

use super::constants::PRAGMA_ORACLE_NAME_FOR_STARKEX;
use super::utils::{only_existing_pairs, sign_data};
use super::AssetOraclePrice;

/// Interval in milliseconds that the channel will update the client with the latest prices.
const CHANNEL_UPDATE_INTERVAL_IN_MS: u64 = 500;

#[derive(Default, Debug, Serialize, Deserialize)]
enum SubscriptionType {
    #[serde(rename = "subscribe")]
    #[default]
    Subscribe,
    #[serde(rename = "unsubscribe")]
    Unsubscribe,
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
struct CurrentSubscription {
    spot_pairs: HashSet<String>,
    perp_pairs: HashSet<String>,
}

impl CurrentSubscription {
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
    #[allow(dead_code)]
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
        let perp_pairs = self.get_subscribed_perp_pairs();
        spot_pairs.extend(perp_pairs);
        spot_pairs
    }
}

#[utoipa::path(
    get,
    path = "/node/v1/data/subscribe",
    responses(
        (
            status = 200,
            description = "Subscribe to a list of entries",
            body = [SubscribeToEntryResponse]
        )
    )
)]
pub async fn subscribe_to_entry(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_channel(socket, state))
}

/// Handle the WebSocket channel.
async fn handle_channel(mut socket: WebSocket, state: AppState) {
    let waiting_duration = Duration::from_millis(CHANNEL_UPDATE_INTERVAL_IN_MS);
    let mut update_interval = interval(waiting_duration);
    let mut subscription: CurrentSubscription = Default::default();

    loop {
        tokio::select! {
            Some(msg) = socket.recv() => {
                if let Ok(Message::Text(text)) = msg {
                    handle_message_received(&mut socket, &state, &mut subscription, text).await;
                }
            },
            _ = update_interval.tick() => {
                match send_median_entries(&mut socket, &state, &subscription).await {
                    Ok(_) => {},
                    Err(_) => break
                };
            }
        }
    }
}

/// Handle the message received from the client.
/// Subscribe or unsubscribe to the pairs requested.
async fn handle_message_received(
    socket: &mut WebSocket,
    state: &AppState,
    subscription: &mut CurrentSubscription,
    message: String,
) {
    if let Ok(subscription_msg) = serde_json::from_str::<SubscriptionRequest>(&message) {
        match subscription_msg.msg_type {
            SubscriptionType::Subscribe => {
                let (existing_spot_pairs, existing_perp_pairs, _) =
                    only_existing_pairs(&state.timescale_pool, subscription_msg.pairs).await;
                subscription.add_spot_pairs(existing_spot_pairs);
                subscription.add_perp_pairs(existing_perp_pairs);
            }
            SubscriptionType::Unsubscribe => {
                let (existing_spot_pairs, existing_perp_pairs, _) =
                    only_existing_pairs(&state.timescale_pool, subscription_msg.pairs).await;
                subscription.remove_spot_pairs(&existing_spot_pairs);
                subscription.remove_perp_pairs(&existing_perp_pairs);
            }
        };
        // We send an ack message to the client with the subscribed pairs (so
        // the client knows which pairs are successfully subscribed).
        if let Ok(ack_message) = serde_json::to_string(&SubscriptionAck {
            msg_type: subscription_msg.msg_type,
            pairs: subscription.get_fmt_subscribed_pairs(),
        }) {
            if socket.send(Message::Text(ack_message)).await.is_err() {
                let error_msg = "Message received but could not send ack message.";
                send_error_message(socket, error_msg).await;
            }
        } else {
            let error_msg = "Could not serialize ack message.";
            send_error_message(socket, error_msg).await;
        }
    } else {
        let error_msg = "Invalid message type. Please check the documentation for more info.";
        send_error_message(socket, error_msg).await;
    }
}

/// Send the current median entries to the client.
async fn send_median_entries(
    socket: &mut WebSocket,
    state: &AppState,
    subscription: &CurrentSubscription,
) -> Result<(), EntryError> {
    if subscription.is_empty() {
        return Ok(());
    }
    let response = match get_subscribed_pairs_medians(state, subscription).await {
        Ok(response) => response,
        Err(e) => {
            send_error_message(socket, &e.to_string()).await;
            return Err(e);
        }
    };

    if let Ok(json_response) = serde_json::to_string(&response) {
        if socket.send(Message::Text(json_response)).await.is_err() {
            send_error_message(socket, "Could not send prices.").await;
        }
    } else {
        send_error_message(socket, "Could not serialize prices.").await;
    }
    Ok(())
}

/// Get the current median entries for the subscribed pairs and sign them as Pragma.
async fn get_subscribed_pairs_medians(
    state: &AppState,
    subscription: &CurrentSubscription,
) -> Result<SubscribeToEntryResponse, EntryError> {
    // 1. Get spot median entries.
    let mut median_entries = get_current_median_entries_with_components(
        &state.timescale_pool,
        &subscription.get_subscribed_spot_pairs(),
    )
    .await
    .map_err(|e| e.to_entry_error(&subscription.get_subscribed_spot_pairs().join(",")))?;

    // 2. Get perp median entries.
    // TODO: Implement perp median entries.
    median_entries.extend(vec![]);

    // 3. Sign all the median entries.
    let mut response: SubscribeToEntryResponse = Default::default();
    let now = chrono::Utc::now().timestamp();
    for entry in median_entries {
        let median_price = entry.median_price.clone();
        let mut oracle_price: AssetOraclePrice = entry
            .try_into()
            .map_err(|_| EntryError::InternalServerError)?;

        let signature = sign_median_price(
            &state.pragma_signer,
            &oracle_price.global_asset_id,
            now as u64,
            median_price,
        )?;

        oracle_price.signature = signature;
        response.oracle_prices.push(oracle_price);
    }
    // Timestamp in seconds.
    response.timestamp_s = now.to_string();

    // 4. Return the response.
    Ok(response)
}

/// Sign the median price with the passed signer and return the signature 0x prefixed.
fn sign_median_price(
    signer: &SigningKey,
    asset_id: &str,
    timestamp: u64,
    median_price: BigDecimal,
) -> Result<String, EntryError> {
    let hash_to_sign = get_entry_hash(
        PRAGMA_ORACLE_NAME_FOR_STARKEX,
        asset_id,
        timestamp,
        &median_price,
    )
    .map_err(|_| EntryError::InternalServerError)?;
    let signature = sign_data(signer, hash_to_sign).map_err(EntryError::InvalidSigner)?;
    Ok(format!("0x{:}", signature))
}

/// Send an error message to the client.
/// (Does not close the connection)
async fn send_error_message(socket: &mut WebSocket, error: &str) {
    let error_msg = json!({ "error": error }).to_string();
    let _ = socket.send(Message::Text(error_msg)).await;
}
