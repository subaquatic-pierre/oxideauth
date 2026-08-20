use std::sync::Arc;

use tracing::info;
use uuid::Uuid;

use crate::cache::{
    entities::client_auth::ClientAuthCache,
    error::CacheResult,
    traits::{CacheEntity, CacheExecutor, CacheKey},
};

pub struct ClientAuthCacheStore<C: CacheExecutor> {
    chx: Arc<C>,
}

impl<C: CacheExecutor> ClientAuthCacheStore<C> {
    pub fn new(chx: Arc<C>) -> Self {
        Self { chx }
    }

    /// Reads the full cached entity. Returns None on a cache miss.
    pub async fn fetch(&self, key: &CacheKey) -> CacheResult<Option<ClientAuthCache>> {
        let value = self.chx.json_get(key.as_ref(), None).await?;

        Ok(value)
    }

    /// Writes the entity with the given TTL (seconds).
    pub async fn write(&self, entity: &ClientAuthCache, ttl: Option<i64>) -> CacheResult<()> {
        self.chx
            .json_set(entity.key().as_ref(), None, entity, ttl)
            .await?;
        info!(
            credential_id = %entity.credential_id,
            membership_id = %entity.membership_id,
            account_id = %entity.account_id,
            ttl = ?ttl,
            "CLIENT_AUTH_CACHE_HYDRATED"
        );
        Ok(())
    }

    /// Deletes the cached entity. Used on invalidation.
    pub async fn invalidate(&self, credential_id: Uuid) -> CacheResult<i64> {
        let key = ClientAuthCache::new_key(credential_id);
        let res = self
            .chx
            .json_del::<ClientAuthCache>(key.as_ref(), None)
            .await?;

        Ok(res)
    }
}
