use axum::extract::State;
use axum::Json;
use bigdecimal::num_bigint::ToBigInt;

use crate::handlers::entries::GetEntryResponse;
use crate::infra::errors::InfraError;
use crate::infra::repositories::entry_repository::{self, MedianEntry};
use crate::utils::PathExtractor;
use crate::AppState;
use pragma_entities::EntryError;

use super::utils::{compute_median_price_and_time, currency_pair_to_pair_id};

#[utoipa::path(
        get,
        path = "/node/v1/data/{quote}/{base}",
        responses(
            (status = 200, description = "Get median entry successfuly", body = [GetEntryResponse])
        ),
        params(
            ("quote" = String, Path, description = "Quote Asset"),
            ("base" = String, Path, description = "Base Asset")
        ),
    )]
pub async fn get_entry(
    State(state): State<AppState>,
    PathExtractor(pair): PathExtractor<(String, String)>,
) -> Result<Json<GetEntryResponse>, EntryError> {
    tracing::info!("Received get entry request for pair {:?}", pair);
    // Construct pair id
    let pair_id = currency_pair_to_pair_id(&pair.0, &pair.1);

    // Get entries from database with given pair id (only the latest one grouped by publisher)
    let mut entries = entry_repository::get_median_entries(&state.pool, pair_id.clone())
        .await
        .map_err(|db_error| match db_error {
            InfraError::InternalServerError => EntryError::InternalServerError,
            InfraError::NotFound => EntryError::NotFound(pair_id.clone()),
        })?;

    // Error if no entries found
    if entries.is_empty() {
        return Err(EntryError::UnknownPairId(pair_id));
    }

    Ok(Json(adapt_entry_to_entry_response(pair_id, &mut entries)))
}

fn adapt_entry_to_entry_response(
    pair_id: String,
    entries: &mut Vec<MedianEntry>,
) -> GetEntryResponse {
    let (price, timestamp) = compute_median_price_and_time(entries).unwrap_or_default();

    GetEntryResponse {
        pair_id,
        timestamp: timestamp.timestamp() as u64,
        num_sources_aggregated: entries.len(),
        price: price.to_bigint().unwrap().to_str_radix(16),
    }
}
