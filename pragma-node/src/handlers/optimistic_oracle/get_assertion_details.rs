use axum::extract::{Path, State};
use axum::Json;
use crate::AppState;
use crate::infra::repositories::oo_repository::assertions;

use crate::handlers::optimistic_oracle::types::AssertionDetails;

#[utoipa::path(
    get,
    path = "/assertions/{assertion_id}",
    responses(
        (status = 200, description = "Get assertion details successfully", body = AssertionDetails)
    ),
    params(
        ("assertion_id" = String, Path, description = "Unique identifier of the assertion"),
    ),
)]
pub async fn get_assertion_details(
    State(state): State<AppState>,
    Path(assertion_id): Path<String>,
) -> Result<Json<AssertionDetails>, axum::http::StatusCode> {
    let assertion_details = assertions::get_assertion_details(&state.onchain_pool, &assertion_id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(assertion_details))
}