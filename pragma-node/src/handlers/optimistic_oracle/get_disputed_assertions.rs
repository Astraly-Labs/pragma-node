use crate::handlers::optimistic_oracle::types::{
    GetDisputedAssertionsParams, GetDisputedAssertionsResponse,
};
use crate::infra::repositories::oo_repository::assertions;
use crate::AppState;
use axum::extract::{Query, State};
use axum::Json;
use pragma_entities::models::optimistic_oracle_error::OptimisticOracleError;

pub const DEFAULT_LIMIT: u32 = 100;

#[utoipa::path(
    get,
    path = "node/v1/optimistic/disputed-assertions",
    responses(
        (status = 200, description = "Get disputed assertions successfully", body = GetDisputedAssertionsResponse)
    ),
    params(
        ("page" = Option<u32>, Query, description = "Page number for pagination"),
        ("limit" = Option<u32>, Query, description = "Number of items per page"),
    ),
)]
#[tracing::instrument]
pub async fn get_disputed_assertions(
    State(state): State<AppState>,
    Query(params): Query<GetDisputedAssertionsParams>,
) -> Result<Json<GetDisputedAssertionsResponse>, OptimisticOracleError> {
    let page = params.page.unwrap_or(1);
    let page_size = params.limit.unwrap_or(DEFAULT_LIMIT);

    let disputed_assertions =
        assertions::get_disputed_assertions(&state.onchain_pool, page, page_size)
            .await
            .map_err(OptimisticOracleError::from)?;

    let total_count = disputed_assertions.len();
    let total_pages = (total_count as f64 / page_size as f64).ceil() as u32;

    let response = GetDisputedAssertionsResponse {
        disputed_assertions,
        total_count,
        current_page: page,
        total_pages,
    };

    Ok(Json(response))
}
