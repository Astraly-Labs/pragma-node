use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;
use utoipa::ToSchema;

use crate::error::RedisError;

#[derive(Debug, thiserror::Error, ToSchema)]
pub enum MerkleFeedError {
    #[error("internal server error")]
    InternalServerError,
    #[error("could not establish a connection with Redis")]
    RedisConnection,
    #[error("option for instrument {1} not found for block {0}")]
    OptionNotFound(u64, String),
    #[error("merkle tree not found for block {0}")]
    MerkleTreeNotFound(u64),
    #[error("invalid option hash, could not convert to felt: {0}")]
    InvalidOptionHash(String),
    #[error("could not deserialize the redis merkle tree into MerkleTree")]
    TreeDeserialization,
}

impl From<RedisError> for MerkleFeedError {
    fn from(error: RedisError) -> Self {
        match error {
            RedisError::InternalServerError => Self::InternalServerError,
            RedisError::Connection => Self::RedisConnection,
            RedisError::OptionNotFound(block, name) => Self::OptionNotFound(block, name),
            RedisError::MerkleTreeNotFound(block) => Self::MerkleTreeNotFound(block),
            RedisError::InvalidOptionHash(r) => Self::InvalidOptionHash(r),
            RedisError::TreeDeserialization => Self::TreeDeserialization,
        }
    }
}

impl IntoResponse for MerkleFeedError {
    fn into_response(self) -> axum::response::Response {
        let (status, err_msg) = match self {
            Self::InvalidOptionHash(hash) => (
                StatusCode::BAD_REQUEST,
                format!(
                    "Option hash is not a correct 0x prefixed hexadecimal hash: {}",
                    hash
                ),
            ),
            Self::OptionNotFound(block_number, instrument_name) => (
                StatusCode::NOT_FOUND,
                format!(
                    "MerkleFeed option for instrument {} has not been found for block {}",
                    instrument_name, block_number
                ),
            ),
            Self::MerkleTreeNotFound(block_number) => (
                StatusCode::NOT_FOUND,
                format!("MerkleFeed tree not found for block {}", block_number),
            ),
            Self::RedisConnection => (
                StatusCode::SERVICE_UNAVAILABLE,
                "Could not establish a connection with the Redis database".to_string(),
            ),
            Self::TreeDeserialization => (
                StatusCode::INTERNAL_SERVER_ERROR,
                String::from("Internal server error: could not decode Redis merkle tree"),
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                String::from("Internal server error"),
            ),
        };
        (
            status,
            Json(
                json!({"resource":"MerkleFeed", "message": err_msg, "happened_at" : chrono::Utc::now() }),
            ),
        )
            .into_response()
    }
}
