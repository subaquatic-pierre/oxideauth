use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use tracing::info;

use crate::cache::{
    entities::oauth_state::OAuthStateCache,
    error::{CacheError, CacheResult},
    traits::{CacheEntity, CacheExecutor, CacheKey},
};

/// Store for OAuth authorization state.
///
/// Each state is written keyed by its `csrf_token` and deleted when consumed,
/// so a single authorization flow can only complete once.
pub struct OAuthStateCacheStore<C: CacheExecutor> {
    chx: Arc<C>,
    write_success_count: AtomicU64,
    consume_success_count: AtomicU64,
}

impl<C: CacheExecutor> OAuthStateCacheStore<C> {
    pub fn new(chx: Arc<C>) -> Self {
        Self {
            chx,
            write_success_count: AtomicU64::new(0),
            consume_success_count: AtomicU64::new(0),
        }
    }

    /// Persists the OAuth state entity with the given TTL (seconds).
    pub async fn write(&self, entity: &OAuthStateCache, ttl: u64) -> CacheResult<()> {
        let key_map = entity.keys();
        let key = key_map.get("oauth_state").unwrap();
        self.chx.set(key.as_ref(), None, entity, Some(ttl)).await?;
        self.write_success_count.fetch_add(1, Ordering::Relaxed);
        info!(
            csrf_token = %entity.csrf_token,
            "OAUTH_STATE_WRITTEN"
        );
        Ok(())
    }

    /// Fetches the OAuth state for `csrf_token` and deletes it in the same
    /// step, so each state value can be consumed exactly once.
    pub async fn fetch_and_consume(&self, csrf_token: &str) -> CacheResult<OAuthStateCache> {
        let key = CacheKey::new("oxauth", "oauth", csrf_token);
        let entity = self
            .chx
            .get::<OAuthStateCache>(key.as_ref(), None)
            .await?
            .ok_or_else(|| CacheError::NotFound("oauth state not found".into()))?;
        self.chx
            .del::<serde_json::Value>(key.as_ref(), None)
            .await?;
        self.consume_success_count.fetch_add(1, Ordering::Relaxed);
        info!(
            csrf_token = %csrf_token,
            "OAUTH_STATE_CONSUMED"
        );
        Ok(entity)
    }

    pub fn write_success_count(&self) -> u64 {
        self.write_success_count.load(Ordering::Relaxed)
    }

    pub fn consume_success_count(&self) -> u64 {
        self.consume_success_count.load(Ordering::Relaxed)
    }
}
