// https://docs.rs/redis/0.26.1/redis/#async

use axum::extract::{Query, State};
use axum::Json;
use pragma_common::types::block_id::{BlockId, BlockTag};
use pragma_common::types::merkle_tree::MerkleProof;
use pragma_common::types::Network;
use pragma_entities::models::merkle_feed_error::MerkleFeedError;
use serde::{Deserialize, Serialize};
use starknet::core::types::Felt;
use utoipa::{IntoParams, ToResponse, ToSchema};

use crate::infra::redis;
use crate::types::hex_hash::HexHash;
use crate::utils::PathExtractor;
use crate::AppState;

#[derive(Default, Deserialize, IntoParams, ToSchema, Debug)]
pub struct GetMerkleProofQuery {
    pub network: Option<Network>,
    pub block_id: Option<BlockId>,
}

#[derive(Debug, Serialize, Deserialize, ToResponse, ToSchema)]
pub struct GetMerkleProofResponse(pub MerkleProof);

#[utoipa::path(
    get,
    path = "/node/v1/merkle_feeds/proof/{option_hash}",
    responses(
        (status = 200, description = "Get the merkle proof", body = [GetMerkleProofResponse])
    ),
    params(
        ("option_hash" = String, Path, description = "Hexadecimal hash of the option"),
        GetMerkleProofQuery
    ),
)]
#[tracing::instrument]
pub async fn get_merkle_feeds_proof(
    State(state): State<AppState>,
    PathExtractor(option_hex_hash): PathExtractor<HexHash>,
    Query(params): Query<GetMerkleProofQuery>,
) -> Result<Json<GetMerkleProofResponse>, MerkleFeedError> {
    tracing::info!("Received get merkle proof request");
    if state.redis_client.is_none() {
        return Err(MerkleFeedError::RedisConnection);
    }

    let option_hex_hash = option_hex_hash.0;
    let network = params.network.unwrap_or_default();
    let block_id = params.block_id.unwrap_or(BlockId::Tag(BlockTag::Latest));

    let merkle_tree = redis::get_merkle_tree(
        state.redis_client.unwrap(),
        network,
        block_id,
        state.caches.merkle_feeds_tree().clone(),
    )
    .await
    .map_err(MerkleFeedError::from)?;

    let option_felt_hash = Felt::from_hex(&option_hex_hash)
        .map_err(|_| MerkleFeedError::InvalidOptionHash(option_hex_hash.clone()))?;

    let merkle_proof = merkle_tree
        .get_proof(&option_felt_hash)
        .ok_or(MerkleFeedError::MerkleProof(option_hex_hash))?;

    let hexadecimals_proof = MerkleProof::from(merkle_proof);
    Ok(Json(GetMerkleProofResponse(hexadecimals_proof)))
}
