use axum::extract::{Query, State};
use axum::Json;
use pragma_common::types::{Interval, Network};
use pragma_entities::EntryError;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToResponse, ToSchema};

use crate::infra::repositories::onchain_repository::history::{
    get_historical_entries_and_decimals, retry_with_routing, HistoricalEntryRaw,
};
use crate::types::timestamp::TimestampRange;
use crate::utils::{big_decimal_price_to_hex, PathExtractor};
use crate::AppState;

use crate::utils::currency_pair_to_pair_id;

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct GetOnchainHistoryParams {
    pub network: Network,
    pub timestamp: TimestampRange,
    pub chunk_interval: Option<Interval>,
    pub routing: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GetOnchainHistoryEntry {
    pair_id: String,
    timestamp: u64,
    median_price: String,
    decimals: u32,
    nb_sources_aggregated: u32,
}

#[derive(Debug, Serialize, Deserialize, ToResponse, ToSchema)]
pub struct GetOnchainHistoryResponse(pub Vec<GetOnchainHistoryEntry>);

#[utoipa::path(
    get,
    path = "/node/v1/onchain/history/{base}/{quote}",
    responses(
        (status = 200, description = "Get the historical onchain median price", body = GetOnchainHistoryResponse)
    ),
    params(
        ("base" = String, Path, description = "Base Asset"),
        ("quote" = String, Path, description = "Quote Asset"),
        GetOnchainHistoryParams
    ),
)]
pub async fn get_onchain_history(
    State(state): State<AppState>,
    PathExtractor(pair): PathExtractor<(String, String)>,
    Query(params): Query<GetOnchainHistoryParams>,
) -> Result<Json<GetOnchainHistoryResponse>, EntryError> {
    tracing::info!("Received get onchain history request for pair {:?}", pair);
    let pair_id: String = currency_pair_to_pair_id(&pair.0, &pair.1);
    let network = params.network;
    let timestamp_range = params.timestamp.assert_time_is_valid()?;
    let chunk_interval = params.chunk_interval.unwrap_or_default();
    let with_routing = params.routing.unwrap_or(false);

    // We first try to get the historical entries for the selected pair
    let query_result = get_historical_entries_and_decimals(
        &state.onchain_pool,
        &state.offchain_pool,
        &network,
        pair_id.clone(),
        &timestamp_range,
        &chunk_interval,
    )
    .await;

    // If the request worked, we return the entries.
    // If it did not succeed and we have have [with_routing] to true,
    // we try other routes.
    let (raw_entries, decimals) = match query_result {
        Ok((raw_entries, decimals)) => (raw_entries, decimals),
        Err(_) if with_routing => {
            retry_with_routing(
                &state.onchain_pool,
                &state.offchain_pool,
                &network,
                pair_id.clone(),
                &timestamp_range,
                &chunk_interval,
            )
            .await?
        }
        Err(e) => return Err(e.to_entry_error(&pair_id)),
    };

    let response = prepare_response(raw_entries, decimals);
    Ok(Json(response))
}

fn prepare_response(
    raw_entries: Vec<HistoricalEntryRaw>,
    decimals: u32,
) -> GetOnchainHistoryResponse {
    GetOnchainHistoryResponse(
        raw_entries
            .into_iter()
            .map(|entry| raw_entry_to_onchain_history_entry(entry, decimals))
            .collect(),
    )
}

fn raw_entry_to_onchain_history_entry(
    entry: HistoricalEntryRaw,
    decimals: u32,
) -> GetOnchainHistoryEntry {
    GetOnchainHistoryEntry {
        pair_id: entry.pair_id,
        timestamp: (entry.timestamp.and_utc().timestamp() as u64),
        median_price: big_decimal_price_to_hex(&entry.median_price),
        nb_sources_aggregated: (entry.nb_sources_aggregated as u32),
        decimals,
    }
}
