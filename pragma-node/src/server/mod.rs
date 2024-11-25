pub(crate) mod routes;

use axum_tracing_opentelemetry::middleware::{OtelAxumLayer, OtelInResponseLayer};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use utoipa::{
    openapi::{
        security::{ApiKey, ApiKeyValue, SecurityScheme},
        ServerBuilder, ServerVariableBuilder,
    },
    Modify, OpenApi,
};
use utoipauto::utoipauto;

use crate::errors::internal_error;
use crate::{config::Config, server::routes::app_router, AppState};

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

struct ServerAddon;

impl Modify for ServerAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let server_variable = ServerVariableBuilder::new()
            .default_value("api.dev")
            .enum_values(Some(vec!["api.dev", "api.prod"]))
            .build();
        openapi.servers = Some(vec![ServerBuilder::new()
            .url("https://{environment}.pragma.build")
            .parameter("environment", server_variable)
            .build()]);
    }
}

#[tracing::instrument(skip(state))]
pub async fn run_api_server(config: &Config, state: AppState) {
    #[utoipauto(
        paths = "./pragma-node/src, ./pragma-common/src from pragma_common, ./pragma-entities/src from pragma_entities"
    )]
    #[derive(OpenApi)]
    #[openapi(
        modifiers(&SecurityAddon, &ServerAddon),
        tags(
            (name = "pragma-node", description = "Pragma Node API")
        ),
    )]
    struct ApiDoc;

    // Uncomment to generate openapi.json
    // TODO: move to a separate bin
    // let json = ApiDoc::openapi().to_json().unwrap();
    // std::fs::write("openapi.json", json).unwrap();

    let app = app_router::<ApiDoc>(state.clone())
        .with_state(state)
        // Logging so we can see whats going on
        .layer(OtelAxumLayer::default())
        .layer(OtelInResponseLayer)
        // Permissive CORS layer to allow all origins
        .layer(CorsLayer::permissive());

    let host = config.server_host();
    let port = config.server_port();
    let address = format!("{}:{}", host, port);
    let socket_addr: SocketAddr = address.parse().unwrap();
    let listener = tokio::net::TcpListener::bind(socket_addr)
        .await
        .expect("Invalid API server address.");

    tracing::info!("🚀 API started at http://{}", socket_addr);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(internal_error)
    .unwrap();
}
