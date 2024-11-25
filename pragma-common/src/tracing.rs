use std::env;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::prelude::*;
use tracing_subscriber::Layer;

pub fn init_tracing(service_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let axum_layer = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        "example_tracing_aka_logging=debug,tower_http=debug,axum::rejection=trace".into()
    });

    let fmt_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_target(false)
        .pretty();

    let filter = filter_fn(|metadata| {
        metadata.target() != "hyper" && metadata.level() <= &tracing::Level::DEBUG
    });

    let mut layers: Vec<Box<dyn Layer<_> + Send + Sync>> = vec![
        Box::new(fmt_layer.with_filter(filter.clone())),
        Box::new(axum_layer.with_filter(filter.clone())),
    ];

    // Check if the Axiom token is set
    if env::var("AXIOM_TOKEN").is_ok() {
        if let Ok(axiom_layer) = tracing_axiom::builder_with_env(service_name)?
            .with_dataset("pragma-node")?
            .build()
        {
            layers.push(Box::new(axiom_layer.with_filter(filter)));
        }
    }

    tracing_subscriber::registry().with(layers).try_init()?;

    Ok(())
}
