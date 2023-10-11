use axum::extract::State;
use axum::Json;

use crate::domain::models::entry::{EntryError, EntryModel};
use crate::handlers::entries::EntryResponse;
use crate::infra::errors::InfraError;
use crate::infra::repositories::entry_repository;
use crate::utils::PathExtractor;
use crate::AppState;

/// Converts a currency pair to a pair id.
fn currency_pair_to_pair_id(quote: &str, base: &str) -> String {
    format!("{}/{}", quote.to_uppercase(), base.to_uppercase())
}

pub async fn get_entry(
    State(state): State<AppState>,
    PathExtractor(pair): PathExtractor<(String, String)>,
) -> Result<Json<EntryResponse>, EntryError> {
    let pair_id = currency_pair_to_pair_id(&pair.0, &pair.1);
    let entry = entry_repository::get(&state.pool, &pair_id)
        .await
        .map_err(|db_error| match db_error {
            InfraError::InternalServerError => EntryError::InternalServerError,
            InfraError::NotFound => EntryError::NotFound(pair_id),
        })?;

    Ok(Json(adapt_entry_to_entry_response(entry)))
}

fn adapt_entry_to_entry_response(entry: EntryModel) -> EntryResponse {
    EntryResponse {
        pair_id: entry.pair_id,
        timestamp: entry.timestamp,
        num_sources_aggregated: 0, // TODO: add real value
        price: entry.price,
    }
}
