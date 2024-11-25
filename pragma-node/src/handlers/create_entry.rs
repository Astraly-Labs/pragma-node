use axum::extract::{self, State};
use axum::Json;
use chrono::{DateTime, Utc};
use pragma_entities::{EntryError, NewEntry, PublisherError};
use serde::{Deserialize, Serialize};
use starknet::core::types::Felt;
use utoipa::{ToResponse, ToSchema};

use crate::config::config;
use crate::infra::kafka;
use crate::infra::repositories::publisher_repository;
use crate::types::entries::Entry;
use crate::utils::{assert_request_signature_is_valid, felt_from_decimal};
use crate::AppState;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateEntryRequest {
    #[schema(value_type = Vec<String>)]
    #[serde(deserialize_with = "felt_from_decimal")]
    pub signature: Vec<Felt>,
    pub entries: Vec<Entry>,
}

impl AsRef<[Felt]> for CreateEntryRequest {
    fn as_ref(&self) -> &[Felt] {
        &self.signature
    }
}

impl AsRef<[Entry]> for CreateEntryRequest {
    fn as_ref(&self) -> &[Entry] {
        &self.entries
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema, ToResponse)]
pub struct CreateEntryResponse {
    number_entries_created: usize,
}

#[utoipa::path(
    post,
    path = "/node/v1/data/publish",
    request_body = CreateEntryRequest,
    responses(
        (status = 200, description = "Entries published successfuly", body = CreateEntryResponse),
        (status = 401, description = "Unauthorized Publisher", body = EntryError)
    )
)]
#[tracing::instrument]
pub async fn create_entries(
    State(state): State<AppState>,
    extract::Json(new_entries): extract::Json<CreateEntryRequest>,
) -> Result<Json<CreateEntryResponse>, EntryError> {
    tracing::info!("Received new entries: {:?}", new_entries);

    let config = config().await;

    if new_entries.entries.is_empty() {
        return Ok(Json(CreateEntryResponse {
            number_entries_created: 0,
        }));
    }

    let publisher_name = new_entries.entries[0].base.publisher.clone();

    let publisher = publisher_repository::get(&state.offchain_pool, publisher_name.clone())
        .await
        .map_err(EntryError::InfraError)?;

    // Check if publisher is active
    publisher.assert_is_active()?;

    // Fetch public key from database
    // TODO: Fetch it from contract
    let public_key = publisher.active_key;
    let public_key = Felt::from_hex(&public_key)
        .map_err(|_| EntryError::PublisherError(PublisherError::InvalidKey(public_key)))?;

    tracing::info!(
        "Retrieved {:?} public key: {:?}",
        publisher_name,
        public_key
    );

    // Fetch account address from database
    // TODO: Cache it
    let account_address = publisher_repository::get(&state.offchain_pool, publisher_name.clone())
        .await
        .map_err(EntryError::InfraError)?
        .account_address;
    let account_address = Felt::from_hex(&account_address)
        .map_err(|_| EntryError::PublisherError(PublisherError::InvalidAddress(account_address)))?;

    tracing::info!(
        "Retrieved {:?} account address: {:?}",
        publisher_name,
        account_address
    );

    let signature = assert_request_signature_is_valid::<CreateEntryRequest, Entry>(
        &new_entries,
        &account_address,
        &public_key,
    )?;

    let new_entries_db = new_entries
        .entries
        .iter()
        .map(|entry| {
            let dt = match DateTime::<Utc>::from_timestamp(entry.base.timestamp as i64, 0) {
                Some(dt) => dt.naive_utc(),
                None => {
                    return Err(EntryError::InvalidTimestamp(format!(
                        "Could not convert {} to DateTime",
                        entry.base.timestamp
                    )))
                }
            };

            Ok(NewEntry {
                pair_id: entry.pair_id.clone(),
                publisher: entry.base.publisher.clone(),
                source: entry.base.source.clone(),
                timestamp: dt,
                publisher_signature: format!("0x{}", signature),
                price: entry.price.into(),
            })
        })
        .collect::<Result<Vec<NewEntry>, EntryError>>()?;

    let data =
        serde_json::to_vec(&new_entries_db).map_err(|e| EntryError::PublishData(e.to_string()))?;

    if let Err(e) = kafka::send_message(config.kafka_topic(), &data, &publisher_name).await {
        tracing::error!("Error sending message to kafka: {:?}", e);
        return Err(EntryError::PublishData(String::from(
            "Error sending message to kafka",
        )));
    };

    Ok(Json(CreateEntryResponse {
        number_entries_created: new_entries.entries.len(),
    }))
}

#[cfg(test)]
mod tests {
    use crate::types::entries::{build_publish_message, BaseEntry, Entry};

    use super::*;
    use rstest::rstest;

    #[rstest]
    fn test_build_publish_message_empty() {
        let entries: Vec<Entry> = vec![];
        let typed_data = build_publish_message(&entries).unwrap();

        assert_eq!(typed_data.primary_type, "Request");
        assert_eq!(typed_data.domain.name, "Pragma");
        assert_eq!(typed_data.domain.version, "1");
        // assert_eq!(typed_data.message.action, "Publish");
        // assert_eq!(typed_data.message.entries, entries);
    }

    #[rstest]
    #[ignore = "TODO: Compute hash with Pragma SDK"]
    fn test_build_publish_message() {
        let entries = vec![Entry {
            base: BaseEntry {
                timestamp: 0,
                source: "source".to_string(),
                publisher: "publisher".to_string(),
            },
            pair_id: "pair_id".to_string(),
            price: 0,
            volume: 0,
        }];
        let typed_data = build_publish_message(&entries).unwrap();

        assert_eq!(typed_data.primary_type, "Request");
        assert_eq!(typed_data.domain.name, "Pragma");
        assert_eq!(typed_data.domain.version, "1");
        // assert_eq!(typed_data.message.action, "Publish");
        // assert_eq!(typed_data.message.entries, entries);

        let msg_hash = typed_data.encode(Felt::ZERO).unwrap().message_hash;
        // Hash computed with the Pragma SDK (python)
        assert_eq!(msg_hash, Felt::from_hex("").unwrap());
    }
}
