use deadpool_diesel::postgres::Pool;
use dotenvy::dotenv;
use pragma_entities::connection::ENV_OFFCHAIN_DATABASE_URL;
use pragma_entities::{
    adapt_infra_error, Entry, FutureEntry, InfraError, NewEntry, NewFutureEntry,
};
use tokio::sync::mpsc;
use tracing::{error, info};
mod config;
mod consumer;
mod error;

#[tokio::main]
#[tracing::instrument]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenv(); // .env file is not present in prod
    pragma_common::tracing::init_tracing("pragma-ingestor")?;
    info!(
        "kafka configuration : hostname={:?}, group_id={}, topic={}",
        config::CONFIG.brokers,
        config::CONFIG.group_id,
        config::CONFIG.topic
    );

    let pool = pragma_entities::connection::init_pool("pragma-ingestor", ENV_OFFCHAIN_DATABASE_URL)
        .expect("cannot connect to offchain database");

    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    tokio::spawn(consumer::consume(tx));
    loop {
        while let Some(payload) = rx.recv().await {
            if let Err(e) = process_payload(&pool, payload).await {
                error!("error while processing payload: {:?}", e);
            }
        }
    }
}

#[tracing::instrument(skip(pool))]
async fn process_payload(pool: &Pool, payload: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
    let decoded_payload = String::from_utf8_lossy(&payload);
    let is_future_entries = decoded_payload.contains("expiration_timestamp");
    if is_future_entries {
        match serde_json::from_slice::<Vec<NewFutureEntry>>(&payload) {
            Ok(future_entries) => {
                if !future_entries.is_empty() {
                    if let Err(e) = insert_future_entries(pool, future_entries).await {
                        error!("error while inserting future entries : {:?}", e);
                    }
                }
            }
            Err(e) => {
                error!("Failed to deserialize payload: {:?}", e);
            }
        }
    } else {
        match serde_json::from_slice::<Vec<NewEntry>>(&payload) {
            Ok(entries) => {
                info!("[SPOT] total of '{}' new entries available.", entries.len());
                if let Err(e) = insert_spot_entries(pool, entries).await {
                    error!("error while inserting entries : {:?}", e);
                }
            }
            Err(e) => {
                error!("Failed to deserialize payload: {:?}", e);
            }
        }
    }
    Ok(())
}

#[tracing::instrument(skip(pool))]
pub async fn insert_spot_entries(
    pool: &Pool,
    new_entries: Vec<NewEntry>,
) -> Result<(), InfraError> {
    let conn = pool.get().await.map_err(adapt_infra_error)?;
    let entries = conn
        .interact(move |conn| Entry::create_many(conn, new_entries))
        .await
        .map_err(adapt_infra_error)?
        .map_err(adapt_infra_error)?;

    for entry in &entries {
        info!(
            "new entry created {} - {}({}) - {}",
            entry.publisher, entry.pair_id, entry.price, entry.source
        );
    }

    Ok(())
}

#[tracing::instrument(skip(pool))]
pub async fn insert_future_entries(
    pool: &Pool,
    new_entries: Vec<NewFutureEntry>,
) -> Result<(), InfraError> {
    let conn = pool.get().await.map_err(adapt_infra_error)?;

    // Double check that we don't have expiration_timestamp set to 0,
    // if we do, we set them to NULL to be extra clear in the database
    // those future entries are perp entries.
    let new_entries = new_entries
        .into_iter()
        .map(|mut entry| {
            if let Some(expiration_timestamp) = entry.expiration_timestamp {
                if expiration_timestamp.and_utc().timestamp() == 0 {
                    entry.expiration_timestamp = None;
                }
            }
            entry
        })
        .collect::<Vec<_>>();

    let len_perp_entries = new_entries
        .iter()
        .filter(|entry| entry.expiration_timestamp.is_none())
        .count();

    info!("[PERP] {} new entries available", len_perp_entries);
    info!(
        "[FUTURE] {} new entries available",
        new_entries.len() - len_perp_entries
    );

    let entries = conn
        .interact(move |conn| FutureEntry::create_many(conn, new_entries))
        .await
        .map_err(adapt_infra_error)?
        .map_err(adapt_infra_error)?;
    for entry in &entries {
        info!(
            "new future entry created {} - {}({}) - {}",
            entry.publisher, entry.pair_id, entry.price, entry.source
        );
    }
    Ok(())
}
