use std::env::current_dir;

use testcontainers::core::IntoContainerPort;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers_modules::postgres::Postgres;

use crate::common::constants::DEFAULT_PG_PORT;

use super::utils::run_migrations;
use super::Timescale;

pub const ONCHAIN_DB_CONTAINER_NAME: &str = "test-onchain-db";

#[rstest::fixture]
pub async fn setup_onchain_db() -> ContainerAsync<Timescale> {
    Postgres::default()
        .with_name("timescale/timescaledb-ha")
        .with_tag("pg14-latest")
        .with_env_var("POSTGRES_DB", "pragma")
        .with_env_var("POSTGRES_PASSWORD", "test-password")
        .with_env_var("TIMESCALEDB_TELEMETRY", "off")
        .with_mapped_port(5433, DEFAULT_PG_PORT.tcp())
        .with_network("pragma-tests-db-network")
        .with_container_name(ONCHAIN_DB_CONTAINER_NAME)
        .start()
        .await
        .unwrap()
}

pub async fn run_onchain_migrations(port: u16) {
    let db_url = format!(
        "postgres://postgres:test-password@localhost:{}/pragma",
        port
    );
    let migrations_folder = current_dir()
        .unwrap()
        .join("..")
        .join("infra")
        .join("pragma-node")
        .join("postgres_migrations");

    run_migrations(&db_url, migrations_folder).await;
}
