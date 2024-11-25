use std::collections::HashMap;
use std::time::Duration;

use moka::future::Cache;
use pragma_common::types::merkle_tree::MerkleTree;

use crate::constants::caches::{
    MERKLE_FEED_TREE_CACHE_TIME_TO_IDLE_IN_SECONDS, MERKLE_FEED_TREE_CACHE_TIME_TO_LIVE_IN_SECONDS,
    PUBLISHERS_UDPATES_CACHE_TIME_TO_IDLE_IN_SECONDS,
    PUBLISHERS_UDPATES_CACHE_TIME_TO_LIVE_IN_SECONDS,
};
use crate::infra::repositories::onchain_repository::publisher::RawPublisherUpdates;

/// Structure responsible of holding our Databases caches.
/// All the caches are initialized empty with their associated time to live in the
/// constants module.
#[derive(Clone)]
pub struct CacheRegistry {
    onchain_publishers_updates: Cache<String, HashMap<String, RawPublisherUpdates>>,
    merkle_feed_tree: Cache<u64, MerkleTree>,
}

impl CacheRegistry {
    /// Initialize all of our caches empty.
    pub fn new() -> Self {
        let onchain_publishers_updates_cache = Cache::builder()
            .time_to_live(Duration::from_secs(
                PUBLISHERS_UDPATES_CACHE_TIME_TO_LIVE_IN_SECONDS,
            )) // 30 minutes
            .time_to_idle(Duration::from_secs(
                PUBLISHERS_UDPATES_CACHE_TIME_TO_IDLE_IN_SECONDS,
            )) // 5 minutes
            .build();

        let merkle_feed_tree_cache = Cache::builder()
            .time_to_live(Duration::from_secs(
                MERKLE_FEED_TREE_CACHE_TIME_TO_LIVE_IN_SECONDS,
            ))
            .time_to_idle(Duration::from_secs(
                MERKLE_FEED_TREE_CACHE_TIME_TO_IDLE_IN_SECONDS,
            ))
            .build();

        CacheRegistry {
            onchain_publishers_updates: onchain_publishers_updates_cache,
            merkle_feed_tree: merkle_feed_tree_cache,
        }
    }

    pub fn onchain_publishers_updates(
        &self,
    ) -> &Cache<String, HashMap<String, RawPublisherUpdates>> {
        &self.onchain_publishers_updates
    }

    pub fn merkle_feeds_tree(&self) -> &Cache<u64, MerkleTree> {
        &self.merkle_feed_tree
    }
}
