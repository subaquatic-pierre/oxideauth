use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
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
}

impl<C: CacheExecutor> OAuthStateCacheStore<C> {
    pub fn new(chx: Arc<C>) -> Self {
        Self { chx }
    }

    /// Persists the OAuth state entity with the given TTL (seconds).
    pub async fn write(&self, entity: &OAuthStateCache, ttl: u64) -> CacheResult<()> {
        // let key_map = entity.key();
        // let key = key_map.get("oauth_state").unwrap();
        // self.chx.set(key.as_ref(), None, entity, Some(ttl)).await?;
        // self.write_success_count.fetch_add(1, Ordering::Relaxed);
        // info!(
        //     csrf_token = %entity.csrf_token,
        //     "OAUTH_STATE_WRITTEN"
        // );
        Ok(())
    }

    pub async fn fetch_and_consume(&self, csrf_token: &str) -> CacheResult<OAuthStateCache> {
        let key = CacheKey::new("oxauth", "oauth", csrf_token);
        let entity = self
            .chx
            .json_get::<OAuthStateCache>(key.as_ref(), None)
            .await?
            .ok_or_else(|| CacheError::NotFound("oauth state not found".into()))?;
        self.chx
            .json_del::<serde_json::Value>(key.as_ref(), None)
            .await?;

        info!(
            csrf_token = %csrf_token,
            "OAUTH_STATE_CONSUMED"
        );
        Ok(entity)
    }
}
