use axum::extract::{Query, State};
use axum::Json;

use pragma_common::types::Network;
use pragma_entities::CheckpointError;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToResponse, ToSchema};

use crate::infra::repositories::entry_repository::get_decimals;
use crate::infra::repositories::onchain_repository::checkpoint::get_checkpoints;
use crate::utils::currency_pair_to_pair_id;
use crate::utils::PathExtractor;
use crate::AppState;

pub const DEFAULT_LIMIT: u64 = 100;
pub const MAX_LIMIT: u64 = 1000;

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct GetOnchainCheckpointsParams {
    pub network: Network,
    pub limit: Option<u64>,
}

impl Default for GetOnchainCheckpointsParams {
    fn default() -> Self {
        Self {
            network: Network::default(),
            limit: Some(DEFAULT_LIMIT),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Checkpoint {
    pub tx_hash: String,
    pub price: String,
    pub timestamp: u64,
    pub sender_address: String,
}

#[derive(Debug, Serialize, Deserialize, ToResponse, ToSchema)]
pub struct GetOnchainCheckpointsResponse(pub Vec<Checkpoint>);

#[utoipa::path(
    get,
    path = "/node/v1/onchain/checkpoints/{base}/{quote}",
    responses(
        (status = 200, description = "Get the onchain checkpoints for a pair", body = GetOnchainCheckpointsResponse)
    ),
    params(
        ("base" = String, Path, description = "Base Asset"),
        ("quote" = String, Path, description = "Quote Asset"),
        GetOnchainCheckpointsParams
    ),
)]
pub async fn get_onchain_checkpoints(
    State(state): State<AppState>,
    PathExtractor(pair): PathExtractor<(String, String)>,
    Query(params): Query<GetOnchainCheckpointsParams>,
) -> Result<Json<GetOnchainCheckpointsResponse>, CheckpointError> {
    tracing::info!("Received get onchain entry request for pair {:?}", pair);

    let pair_id: String = currency_pair_to_pair_id(&pair.0, &pair.1);

    let limit = params.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(CheckpointError::InvalidLimit(limit));
    }

    let decimals = get_decimals(&state.offchain_pool, &pair_id)
        .await
        .map_err(CheckpointError::from)?;

    let checkpoints = get_checkpoints(
        &state.onchain_pool,
        params.network,
        pair_id.clone(),
        decimals,
        limit,
    )
    .await
    .map_err(CheckpointError::from)?;

    if checkpoints.is_empty() {
        return Err(CheckpointError::NotFound);
    }
    Ok(Json(GetOnchainCheckpointsResponse(checkpoints)))
}
