use crate::handlers::optimistic_oracle::types::{GetAssertionsParams, GetAssertionsResponse};
use crate::infra::repositories::oo_repository::assertions;
use crate::AppState;
use axum::extract::{Query, State};
use axum::Json;
use pragma_entities::models::optimistic_oracle_error::OptimisticOracleError;

pub const DEFAULT_LIMIT: u32 = 100;

#[utoipa::path(
    get,
    path = "node/v1/optimistic/assertions",
    responses(
        (status = 200, description = "Get assertions successfully", body = GetAssertionsResponse)
    ),
    params(
        ("status" = Option<String>, Query, description = "Filter by assertion status"),
        ("page" = Option<u32>, Query, description = "Page number for pagination"),
        ("limit" = Option<u32>, Query, description = "Number of items per page"),
    ),
)]
#[tracing::instrument]
pub async fn get_assertions(
    State(state): State<AppState>,
    Query(params): Query<GetAssertionsParams>,
) -> Result<Json<GetAssertionsResponse>, OptimisticOracleError> {
    let page = params.page.unwrap_or(1);
    let page_size = params.limit.unwrap_or(DEFAULT_LIMIT);

    let assertions =
        assertions::get_assertions(&state.onchain_pool, params.status, page, page_size)
            .await
            .map_err(OptimisticOracleError::from)?;

    let total_count = assertions.len();
    let total_pages = (total_count as f64 / page_size as f64).ceil() as u32;

    let response = GetAssertionsResponse {
        assertions,
        total_count: total_count as i64,
        current_page: page,
        total_pages,
    };

    Ok(Json(response))
}
