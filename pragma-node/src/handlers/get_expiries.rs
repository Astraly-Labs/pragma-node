use axum::extract::State;
use axum::Json;
use chrono::NaiveDateTime;

use pragma_entities::EntryError;

use crate::infra::repositories::entry_repository;
use crate::utils::PathExtractor;
use crate::AppState;

use crate::utils::currency_pair_to_pair_id;

#[utoipa::path(
    get,
    path = "/node/v1/data/{base}/{quote}/future_expiries",
    responses(
        (status = 200, description = "Get available future expiries for a pair", body = [Vec<NaiveDateTime>])
    ),
    params(
        ("base" = String, Path, description = "Base Asset"),
        ("quote" = String, Path, description = "Quote Asset"),
    ),
)]
#[tracing::instrument]
pub async fn get_expiries(
    State(state): State<AppState>,
    PathExtractor(pair): PathExtractor<(String, String)>,
) -> Result<Json<Vec<NaiveDateTime>>, EntryError> {
    tracing::info!("Received get expiries for pair {:?}", pair);

    let pair_id = currency_pair_to_pair_id(&pair.0, &pair.1);

    let req_result = entry_repository::get_expiries_list(&state.offchain_pool, pair_id.clone())
        .await
        .map_err(|e| e.to_entry_error(&(pair_id)))?;

    Ok(Json(req_result))
}
