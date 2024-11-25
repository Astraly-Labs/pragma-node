use crate::infra::repositories::oo_repository::assertions;
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use pragma_entities::models::optimistic_oracle_error::OptimisticOracleError;

use crate::handlers::optimistic_oracle::types::AssertionDetails;

#[utoipa::path(
    get,
    path = "node/v1/optimistic/assertions/{assertion_id}",
    responses(
        (status = 200, description = "Get assertion details successfully", body = AssertionDetails)
    ),
    params(
        ("assertion_id" = String, Path, description = "Unique identifier of the assertion"),
    ),
)]
#[tracing::instrument]
pub async fn get_assertion_details(
    State(state): State<AppState>,
    Path(assertion_id): Path<String>,
) -> Result<Json<AssertionDetails>, OptimisticOracleError> {
    let assertion_details = assertions::get_assertion_details(&state.onchain_pool, &assertion_id)
        .await
        .map_err(OptimisticOracleError::from)?;

    Ok(Json(assertion_details))
}
