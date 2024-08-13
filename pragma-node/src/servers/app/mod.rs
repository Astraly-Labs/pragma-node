pub(crate) mod routes;

use std::net::SocketAddr;
use tower_http::{
    cors::CorsLayer,
    trace::{DefaultMakeSpan, TraceLayer},
};
use utoipa::{
    openapi::security::{ApiKey, ApiKeyValue, SecurityScheme},
    Modify, OpenApi,
};
use utoipauto::utoipauto;

use crate::errors::internal_error;
use crate::{config::Config, servers::app::routes::app_router, AppState};

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "api_key",
                SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("x-api-key"))),
            )
        }
    }
}
#[utoipauto(
    paths = "./pragma-node/src, ./pragma-common/src from pragma_common, ./pragma-entities/src from pragma_entities"
)]
#[derive(OpenApi)]
#[openapi(
modifiers(&SecurityAddon),
tags(
    (name = "pragma-node", description = "Pragma Node API")
)
)]
pub struct ApiDoc;

pub async fn run_app_server(config: &Config, state: AppState) {

    let app = app_router::<ApiDoc>(state.clone())
        .with_state(state)
        // Logging so we can see whats going on
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        )
        // Permissive CORS layer to allow all origins
        .layer(CorsLayer::permissive());

    let host = config.server_host();
    let port = config.server_port();
    let address = format!("{}:{}", host, port);
    let socket_addr: SocketAddr = address.parse().unwrap();

    tracing::info!("🚀 API started at http://{}", socket_addr);
    tokio::spawn(async move {
        axum::Server::bind(&socket_addr)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .map_err(internal_error)
            .unwrap()
    });
}
