use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use utoipa::OpenApi as OpenApiT;
use utoipa_swagger_ui::SwaggerUi;

use crate::handlers::merkle_feeds::{
    get_merkle_proof::get_merkle_feeds_proof, get_option::get_merkle_feeds_option,
};
use crate::handlers::onchain::{
    get_checkpoints::get_onchain_checkpoints, get_entry::get_onchain_entry,
    get_history::get_onchain_history, get_publishers::get_onchain_publishers,
    subscribe_to_ohlc::subscribe_to_onchain_ohlc,
};
use crate::handlers::optimistic_oracle::{
    get_assertion_details::get_assertion_details, get_assertions::get_assertions,
    get_disputed_assertions::get_disputed_assertions,
    get_resolved_assertions::get_resolved_assertions,
};
use crate::handlers::{
    create_entries, create_future_entries, get_entry, get_expiries, get_ohlc, get_volatility,
    subscribe_to_entry, subscribe_to_price,
};
use crate::AppState;

pub fn app_router<T: OpenApiT>(state: AppState) -> Router<AppState> {
    let open_api = T::openapi();
    Router::new()
        .merge(SwaggerUi::new("/node/swagger-ui").url("/node/api-docs/openapi.json", open_api))
        .route("/node", get(root))
        .nest("/node/v1/data", data_routes(state.clone()))
        .nest("/node/v1/onchain", onchain_routes(state.clone()))
        .nest("/node/v1/aggregation", aggregation_routes(state.clone()))
        .nest("/node/v1/volatility", volatility_routes(state.clone()))
        .nest("/node/v1/merkle_feeds", merkle_feeds_routes(state.clone()))
        .nest(
            "/node/v1/optimistic",
            optimistic_oracle_routes(state.clone()),
        )
        .fallback(handler_404)
}

async fn root() -> &'static str {
    "Server is running!"
}

async fn handler_404() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        "The requested resource was not found",
    )
}

fn data_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/publish", post(create_entries))
        .route("/publish_future", post(create_future_entries))
        .route("/:base/:quote", get(get_entry))
        .route("/:base/:quote/future_expiries", get(get_expiries))
        .route("/subscribe", get(subscribe_to_entry))
        .route("/price/subscribe", get(subscribe_to_price))
        .with_state(state)
}

fn onchain_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/:base/:quote", get(get_onchain_entry))
        .route("/history/:base/:quote", get(get_onchain_history))
        .route("/checkpoints/:base/:quote", get(get_onchain_checkpoints))
        .route("/publishers", get(get_onchain_publishers))
        .route("/ohlc/subscribe", get(subscribe_to_onchain_ohlc))
        .with_state(state)
}

fn volatility_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/:base/:quote", get(get_volatility))
        .with_state(state)
}

fn aggregation_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/candlestick/:base/:quote", get(get_ohlc))
        .with_state(state)
}

fn merkle_feeds_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/proof/:option_hash", get(get_merkle_feeds_proof))
        .route("/options/:instrument", get(get_merkle_feeds_option))
        .with_state(state)
}

fn optimistic_oracle_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/assertions/:assertion_id", get(get_assertion_details))
        .route("/assertions", get(get_assertions))
        .route("/disputed-assertions", get(get_disputed_assertions))
        .route("/resolved-assertions", get(get_resolved_assertions))
        .with_state(state)
}
