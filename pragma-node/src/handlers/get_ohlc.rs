use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::{ToResponse, ToSchema};

use crate::handlers::Interval;
use crate::infra::repositories::entry_repository::{self, OHLCEntry};
use crate::utils::PathExtractor;
use crate::AppState;
use pragma_entities::EntryError;

use super::GetEntryParams;
use crate::utils::currency_pair_to_pair_id;

#[derive(Debug, Serialize, Deserialize, ToSchema, ToResponse)]
pub struct GetOHLCResponse {
    pair_id: String,
    data: Vec<OHLCEntry>,
}

#[utoipa::path(
        get,
        path = "/node/v1/aggregation/candlestick/{base}/{quote}",
        responses(
            (status = 200, description = "Get OHLC data successfuly", body = [GetOHLCResponse])
        ),
        params(
            ("base" = String, Path, description = "Base Asset"),
            ("quote" = String, Path, description = "Quote Asset"),
            GetEntryParams,
        ),
    )]
pub async fn get_ohlc(
    State(state): State<AppState>,
    PathExtractor(pair): PathExtractor<(String, String)>,
    Query(params): Query<GetEntryParams>,
) -> Result<Json<GetOHLCResponse>, EntryError> {
    tracing::info!("Received get entry request for pair {:?}", pair);
    // Construct pair id
    let pair_id = currency_pair_to_pair_id(&pair.0, &pair.1);

    let now = chrono::Utc::now().timestamp();

    let timestamp = if let Some(timestamp) = params.timestamp {
        timestamp
    } else {
        now
    };

    let interval = if let Some(interval) = params.interval {
        interval
    } else {
        Interval::OneMinute
    };

    // Validate given timestamp
    if timestamp > now {
        return Err(EntryError::InvalidTimestamp(format!(
            "Timestamp is in the future: {timestamp}"
        )));
    }

    let entries =
        entry_repository::get_ohlc(&state.offchain_pool, pair_id.clone(), interval, timestamp)
            .await
            .map_err(|db_error| db_error.to_entry_error(&pair_id))?;

    Ok(Json(adapt_entry_to_entry_response(pair_id, &entries)))
}

fn adapt_entry_to_entry_response(pair_id: String, entries: &[OHLCEntry]) -> GetOHLCResponse {
    GetOHLCResponse {
        pair_id,
        data: entries.to_vec(),
    }
}
