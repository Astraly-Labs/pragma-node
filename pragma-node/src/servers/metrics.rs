use axum::body::Body;
use axum::http::{Response, StatusCode};
use axum::response::IntoResponse;
use axum::Server;
use axum::{routing::get, Router};
use prometheus::{Encoder, TextEncoder};
use std::net::SocketAddr;

use crate::config::Config;
use crate::metrics::MetricsRegistry;

pub async fn run_metrics_server(config: &Config, metrics_registry: MetricsRegistry) {
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/metrics", get(move || metrics_handler(metrics_registry)));

    let host = config.server_host();
    let port = config.metrics_port();

    let address = format!("{}:{}", host, port);
    let socket_addr: SocketAddr = address.parse().unwrap();

    tracing::info!("🖨  Metrics available at http://{}/metrics", socket_addr);
    Server::bind(&socket_addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn root_handler() -> impl IntoResponse {
    let html_content = "<a href=\"/metrics\">/metrics</a>";
    Response::builder()
        .header("Content-Type", "text/html")
        .body(Body::from(html_content))
        .unwrap()
}

async fn metrics_handler(metrics_registry: MetricsRegistry) -> impl IntoResponse {
    let mut buffer = vec![];
    let encoder = TextEncoder::new();

    let metric_families = metrics_registry.registry().gather();
    encoder.encode(&metric_families, &mut buffer).unwrap();

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", encoder.format_type())
        .body(Body::from(buffer))
        .unwrap()
}
