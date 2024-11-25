use axum::extract::{Query, State};
use axum::Json;
use chrono::{DateTime, NaiveDateTime, Utc};

use pragma_common::types::{AggregationMode, DataType, Interval};
use pragma_entities::EntryError;
use serde::{Deserialize, Serialize};
use utoipa::{ToResponse, ToSchema};

use crate::infra::repositories::entry_repository::{self, MedianEntry};
use crate::utils::PathExtractor;
use crate::AppState;

use crate::utils::{big_decimal_price_to_hex, currency_pair_to_pair_id};

use super::GetEntryParams;

#[derive(Default, Clone, Debug)]
pub struct RoutingParams {
    pub interval: Interval,
    pub timestamp: i64,
    pub aggregation_mode: AggregationMode,
    pub data_type: DataType,
    pub expiry: String,
}

impl TryFrom<GetEntryParams> for RoutingParams {
    type Error = EntryError;

    fn try_from(params: GetEntryParams) -> Result<Self, Self::Error> {
        let now = chrono::Utc::now().timestamp();

        let timestamp = if let Some(timestamp) = params.timestamp {
            timestamp
        } else {
            now
        };

        if timestamp > now {
            return Err(EntryError::InvalidTimestamp(format!(
                "Timestamp is in the future: {timestamp}"
            )));
        }

        let interval = if let Some(interval) = params.interval {
            interval
        } else {
            Interval::TwoHours
        };

        let aggregation_mode = if let Some(aggregation_mode) = params.aggregation {
            aggregation_mode
        } else {
            AggregationMode::Twap
        };

        let data_type = if let Some(entry_type) = params.entry_type {
            DataType::from(entry_type)
        } else {
            DataType::SpotEntry
        };

        let expiry = if let Some(expiry) = params.expiry {
            let expiry_dt = NaiveDateTime::parse_from_str(&expiry, "%Y-%m-%dT%H:%M:%S")
                .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
            match expiry_dt {
                Ok(expiry_dt) => expiry_dt.format("%Y-%m-%d %H:%M:%S%:z").to_string(),
                Err(_) => return Err(EntryError::InvalidExpiry),
            }
        } else {
            String::default()
        };

        Ok(RoutingParams {
            interval,
            timestamp,
            aggregation_mode,
            data_type,
            expiry,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema, ToResponse)]
pub struct GetEntryResponse {
    num_sources_aggregated: usize,
    pair_id: String,
    price: String,
    timestamp: u64,
    decimals: u32,
}

#[utoipa::path(
    get,
    path = "/node/v1/data/{base}/{quote}",
    responses(
        (status = 200, description = "Get median entry successfuly", body = [GetEntryResponse])
    ),
    params(
        ("base" = String, Path, description = "Base Asset"),
        ("quote" = String, Path, description = "Quote Asset"),
        GetEntryParams,
    ),
)]
pub async fn get_entry(
    State(state): State<AppState>,
    PathExtractor(pair): PathExtractor<(String, String)>,
    Query(params): Query<GetEntryParams>,
) -> Result<Json<GetEntryResponse>, EntryError> {
    tracing::info!("Received get entry request for pair {:?}", pair);

    let is_routing = params.routing.unwrap_or(false);

    let routing_params = RoutingParams::try_from(params)?;

    let pair_id = currency_pair_to_pair_id(&pair.0, &pair.1);

    let (entry, decimals) = entry_repository::routing(
        &state.offchain_pool,
        is_routing,
        pair_id.clone(),
        routing_params,
    )
    .await
    .map_err(|e| e.to_entry_error(&(pair_id)))?;

    let last_updated_timestamp: NaiveDateTime =
        entry_repository::get_last_updated_timestamp(&state.offchain_pool, pair_id.to_owned())
            .await?
            .unwrap_or(entry.time);

    Ok(Json(adapt_entry_to_entry_response(
        pair_id,
        &entry,
        decimals,
        last_updated_timestamp,
    )))
}

fn adapt_entry_to_entry_response(
    pair_id: String,
    entry: &MedianEntry,
    decimals: u32,
    last_updated_timestamp: NaiveDateTime,
) -> GetEntryResponse {
    GetEntryResponse {
        pair_id,
        timestamp: last_updated_timestamp.and_utc().timestamp_millis() as u64,
        num_sources_aggregated: entry.num_sources as usize,
        price: big_decimal_price_to_hex(&entry.median_price),
        decimals,
    }
}
