pub mod checkpoints;
pub mod ohlc;
pub mod publishers;

use axum::extract::{Query, State};
use axum::Json;
use bigdecimal::BigDecimal;

use pragma_entities::EntryError;

use crate::handlers::entries::{GetOnchainParams, GetOnchainResponse};
use crate::infra::repositories::onchain_repository::{
    get_last_updated_timestamp, get_sources_and_aggregate,
};
use crate::utils::PathExtractor;
use crate::AppState;

use super::utils::currency_pair_to_pair_id;
use super::OnchainEntry;

#[utoipa::path(
    get,
    path = "/node/v1/onchain/{base}/{quote}",
    responses(
        (status = 200, description = "Get the onchain price", body = GetOnchainResponse)
    ),
    params(
        ("base" = String, Path, description = "Base Asset"),
        ("quote" = String, Path, description = "Quote Asset"),
        ("network" = Network, Query, description = "Network"),
        ("aggregation" = AggregationMode, Query, description = "Aggregation Mode"),
        ("timestamp" = Option<u64>, Query, description = "Timestamp")
    ),
)]
pub async fn get_onchain(
    State(state): State<AppState>,
    PathExtractor(pair): PathExtractor<(String, String)>,
    Query(params): Query<GetOnchainParams>,
) -> Result<Json<GetOnchainResponse>, EntryError> {
    tracing::info!("Received get onchain entry request for pair {:?}", pair);

    let pair_id: String = currency_pair_to_pair_id(&pair.0, &pair.1);
    let now = chrono::Utc::now().naive_utc().and_utc().timestamp() as u64;
    let timestamp = if let Some(timestamp) = params.timestamp {
        timestamp
    } else {
        now
    };

    let (aggregated_price, sources) = get_sources_and_aggregate(
        &state.postgres_pool,
        params.network,
        pair_id.clone(),
        timestamp,
        params.aggregation,
    )
    .await
    .map_err(|_| EntryError::InternalServerError)?;

    // TODO(akhercha): ⚠ gives different result than onchain oracle
    // let last_updated_timestamp = sources[0].timestamp;
    let last_updated_timestamp =
        get_last_updated_timestamp(&state.postgres_pool, params.network, pair_id.clone())
            .await
            .map_err(|_| EntryError::InternalServerError)?;

    Ok(Json(adapt_entries_to_onchain_response(
        pair_id,
        // TODO(akhercha): fetch decimals in currencies table
        8,
        sources,
        aggregated_price,
        last_updated_timestamp,
    )))
}

fn adapt_entries_to_onchain_response(
    pair_id: String,
    decimals: u32,
    sources: Vec<OnchainEntry>,
    aggregated_price: BigDecimal,
    last_updated_timestamp: u64,
) -> GetOnchainResponse {
    GetOnchainResponse {
        pair_id,
        last_updated_timestamp,
        // TODO(akhercha): Format the price
        price: aggregated_price.to_string(),
        decimals,
        nb_sources_aggregated: sources.len() as u32,
        // Only asset type used for now is Crypto
        asset_type: "Crypto".to_string(),
        components: sources,
    }
}
