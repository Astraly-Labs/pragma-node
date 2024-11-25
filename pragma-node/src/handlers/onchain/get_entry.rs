use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::Json;
use bigdecimal::BigDecimal;
use pragma_common::types::{AggregationMode, Interval, Network};
use pragma_entities::EntryError;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToResponse, ToSchema};

use crate::infra::repositories::onchain_repository::entry::{
    get_last_updated_timestamp, get_variations, routing, OnchainRoutingArguments,
};
use crate::utils::{big_decimal_price_to_hex, PathExtractor};
use crate::AppState;

use crate::utils::currency_pair_to_pair_id;

#[derive(Debug, Default, Deserialize, IntoParams, ToSchema)]
pub struct GetOnchainEntryParams {
    pub network: Network,
    pub aggregation: Option<AggregationMode>,
    pub routing: Option<bool>,
    pub timestamp: Option<i64>,
    pub components: Option<bool>,
    pub variations: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct OnchainEntry {
    pub publisher: String,
    pub source: String,
    pub price: String,
    pub tx_hash: String,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, ToResponse)]
pub struct GetOnchainEntryResponse {
    pair_id: String,
    last_updated_timestamp: u64,
    price: String,
    decimals: u32,
    nb_sources_aggregated: u32,
    asset_type: String,
    components: Option<Vec<OnchainEntry>>,
    variations: Option<HashMap<Interval, f32>>,
}

#[utoipa::path(
    get,
    path = "/node/v1/onchain/{base}/{quote}",
    responses(
        (status = 200, description = "Get the onchain entry", body = GetOnchainEntryResponse)
    ),
    params(
        ("base" = String, Path, description = "Base Asset"),
        ("quote" = String, Path, description = "Quote Asset"),
        GetOnchainEntryParams
    ),
)]
pub async fn get_onchain_entry(
    State(state): State<AppState>,
    PathExtractor(pair): PathExtractor<(String, String)>,
    Query(params): Query<GetOnchainEntryParams>,
) -> Result<Json<GetOnchainEntryResponse>, EntryError> {
    tracing::info!("Received get onchain entry request for pair {:?}", pair);
    let pair_id: String = currency_pair_to_pair_id(&pair.0, &pair.1);
    let with_components = params.components.unwrap_or(true);
    let with_variations = params.variations.unwrap_or(true);

    let now = chrono::Utc::now().timestamp();
    let timestamp = if let Some(timestamp) = params.timestamp {
        timestamp
    } else {
        now
    };

    let routing_arguments = OnchainRoutingArguments {
        pair_id: pair_id.clone(),
        network: params.network,
        timestamp: (timestamp as u64),
        aggregation_mode: params.aggregation.unwrap_or_default(),
        is_routing: params.routing.unwrap_or(false),
    };

    let raw_data = routing(&state.onchain_pool, &state.offchain_pool, routing_arguments)
        .await
        .map_err(|db_error| db_error.to_entry_error(&pair_id))?;

    let entry = raw_data
        .first()
        .ok_or_else(|| EntryError::NotFound(pair_id.to_string()))?;

    let last_updated_timestamp =
        get_last_updated_timestamp(&state.onchain_pool, params.network, entry.pair_used.clone())
            .await
            .map_err(|db_error| db_error.to_entry_error(&pair_id))?;

    let variations = if with_variations {
        Some(
            get_variations(&state.onchain_pool, params.network, pair_id.clone())
                .await
                .map_err(|db_error| db_error.to_entry_error(&pair_id))?,
        )
    } else {
        None
    };

    Ok(Json(adapt_entries_to_onchain_response(
        pair_id.clone(),
        entry.decimal,
        entry.sources.clone(),
        entry.price.clone(),
        last_updated_timestamp,
        variations,
        with_components,
    )))
}

fn adapt_entries_to_onchain_response(
    pair_id: String,
    decimals: u32,
    sources: Vec<OnchainEntry>,
    aggregated_price: BigDecimal,
    last_updated_timestamp: u64,
    variations: Option<HashMap<Interval, f32>>,
    with_components: bool,
) -> GetOnchainEntryResponse {
    GetOnchainEntryResponse {
        pair_id,
        last_updated_timestamp,
        price: big_decimal_price_to_hex(&aggregated_price),
        decimals,
        nb_sources_aggregated: sources.len() as u32,
        // Only asset type used for now is Crypto
        asset_type: "Crypto".to_string(),
        components: with_components.then_some(sources),
        variations,
    }
}
