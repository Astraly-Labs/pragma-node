mod caches;
mod config;
mod constants;
mod errors;
mod handlers;
mod infra;
mod metrics;
mod servers;
mod types;
mod utils;

use dotenvy::dotenv;
use std::sync::Arc;

use caches::CacheRegistry;
use deadpool_diesel::postgres::Pool;
use starknet::signers::SigningKey;

use pragma_entities::connection::{ENV_OFFCHAIN_DATABASE_URL, ENV_ONCHAIN_DATABASE_URL};

use crate::config::config;
use crate::metrics::MetricsRegistry;
use crate::utils::PragmaSignerBuilder;
use types::ws::metrics::WsMetrics;

#[derive(Clone)]
pub struct AppState {
    // Databases pools
    offchain_pool: Pool,
    onchain_pool: Pool,
    // Redis connection
    #[allow(dead_code)]
    redis_client: Option<Arc<redis::Client>>,
    // Database caches
    caches: Arc<CacheRegistry>,
    // Pragma Signer used for StarkEx signing
    pragma_signer: Option<SigningKey>,
    // Metrics
    ws_metrics: Arc<WsMetrics>,
}

#[tokio::main]
#[tracing::instrument]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    pragma_common::tracing::init_tracing("pragma-node")?;

    let config = config().await;

    // Init the database pools
    let offchain_pool =
        pragma_entities::connection::init_pool("pragma-node-api", ENV_OFFCHAIN_DATABASE_URL)
            .expect("can't init offchain database pool");
    pragma_entities::db::run_migrations(&offchain_pool).await;
    let onchain_pool =
        pragma_entities::connection::init_pool("pragma-node-api", ENV_ONCHAIN_DATABASE_URL)
            .expect("can't init onchain database pool");

    // Init the database caches
    let caches = CacheRegistry::new();

    // Build the pragma signer
    let signer_builder = if config.is_production_mode() {
        PragmaSignerBuilder::new().production_mode()
    } else {
        PragmaSignerBuilder::new().non_production_mode()
    };
    let pragma_signer = signer_builder.build().await;

    // Init the redis client - Optionnal, only for endpoints that interact with Redis,
    // i.e just the Merkle Feeds endpoint for now.
    // TODO(akhercha): See with Hithem for production mode
    let redis_client = match pragma_entities::connection::init_redis_client(
        config.redis_host(),
        config.redis_port(),
    ) {
        Ok(client) => Some(Arc::new(client)),
        Err(_) => {
            tracing::warn!(
                "⚠ Could not create the Redis client. Merkle feeds endpoints won't work."
            );
            None
        }
    };

    // Create the Metrics registry
    let metrics_registry = MetricsRegistry::new();
    let ws_metrics = WsMetrics::new(&metrics_registry).expect("Failed to create WsMetrics");

    let state = AppState {
        offchain_pool,
        onchain_pool,
        redis_client,
        caches: Arc::new(caches),
        pragma_signer,
        ws_metrics: Arc::new(ws_metrics),
    };

    tokio::join!(
        servers::app::run_app_server(config, state),
        servers::metrics::run_metrics_server(config, metrics_registry)
    );

    // Ensure that the tracing provider is shutdown correctly
    opentelemetry::global::shutdown_tracer_provider();

    Ok(())
}
