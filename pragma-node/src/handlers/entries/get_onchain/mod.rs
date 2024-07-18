pub mod checkpoints;
pub mod ohlc;
pub mod publishers;

use axum::extract::{Query, State};
use axum::Json;
use bigdecimal::BigDecimal;
use pragma_entities::EntryError;

use crate::handlers::entries::{GetOnchainParams, GetOnchainResponse};
use crate::infra::repositories::entry_repository::get_decimals;
use crate::infra::repositories::onchain_repository::{get_last_updated_timestamp, routing};
use crate::utils::{format_bigdecimal_price, PathExtractor};
use crate::AppState;

use super::OnchainEntry;
use crate::utils::currency_pair_to_pair_id;

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
        ("aggregation" = Option<AggregationMode>, Query, description = "Aggregation Mode"),
        ("timestamp" = Option<u64>, Query, description = "Timestamp")
    ),
)]
pub async fn get_onchain(
    State(state): State<AppState>,
    PathExtractor(pair): PathExtractor<(String, String)>,
    Query(params): Query<GetOnchainParams>,
) -> Result<Json<GetOnchainResponse>, EntryError> {
    tracing::info!("Received get onchain entry request for pair {:?}", pair);
    let is_routing = params.routing.unwrap_or(false);

    let pair_id: String = currency_pair_to_pair_id(&pair.0, &pair.1);
    let now = chrono::Utc::now().timestamp() as u64;
    let aggregation_mode = params.aggregation.unwrap_or_default();
    let timestamp = if let Some(timestamp) = params.timestamp {
        if timestamp > now {
            return Err(EntryError::InvalidTimestamp);
        }
        timestamp
    } else {
        now
    };

    let (aggregated_price, sources, pair_used, price_decimals) = routing(
        &state.onchain_pool,
        &state.offchain_pool,
        params.network,
        pair_id.clone(),
        timestamp,
        aggregation_mode,
        is_routing,
    )
    .await
    .map_err(|db_error| db_error.to_entry_error(&pair_id))?;

    let mut last_updated_timestamp = 0;
    for pair in pair_used {
        let last_timestamp = get_last_updated_timestamp(&state.onchain_pool, params.network, pair)
            .await
            .map_err(|db_error| db_error.to_entry_error(&pair_id))?;
        last_updated_timestamp = if last_timestamp > last_updated_timestamp {
            last_timestamp
        } else {
            last_updated_timestamp
        };
    }

    let decimals = match price_decimals {
        Some(dec) => dec,
        None => get_decimals(&state.offchain_pool, &pair_id)
            .await
            .map_err(|db_error| db_error.to_entry_error(&pair_id))?,
    };

    Ok(Json(adapt_entries_to_onchain_response(
        pair_id,
        decimals,
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
        price: format_bigdecimal_price(aggregated_price, decimals),
        decimals,
        nb_sources_aggregated: sources.len() as u32,
        // Only asset type used for now is Crypto
        asset_type: "Crypto".to_string(),
        components: sources,
    }
}
