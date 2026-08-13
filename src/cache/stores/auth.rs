use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use tracing::{error, info};
use uuid::Uuid;

use crate::cache::{
    entities::auth::{AuthCache, AuthScopeCache},
    error::{CacheError, CacheResult},
    traits::{CacheEntity, CacheExecutor, CacheKey},
};

pub struct AuthCacheStore<C: CacheExecutor> {
    chx: Arc<C>,
}

impl<C: CacheExecutor> AuthCacheStore<C> {
    pub fn new(chx: Arc<C>) -> Self {
        Self { chx }
    }

    /// Pipeline-reads all keys for the entity. Returns None if any key is missing
    /// (all-or-nothing contract). Returns Some(hydrated_entity) if all keys present.
    pub async fn fetch(&self, key: &CacheKey) -> CacheResult<Option<AuthCache>> {
        let value = self.chx.json_get(key.as_ref(), None).await?;

        Ok(value)
    }

    /// Writes all keys for the entity with the given TTL (seconds).
    pub async fn write(&self, entity: &AuthCache, ttl: Option<u64>) -> CacheResult<()> {
        self.chx
            .json_set(entity.key().as_ref(), None, entity, ttl)
            .await?;
        info!(
            membership_id = %entity.mem_id,
            account_id = %entity.acc_id,
            "AUTH_CACHE_HYDRATED"
        );
        Ok(())
    }

    /// Deletes all keys for the entity. Used on invalidation.
    pub async fn invalidate(&self, mem_id: Uuid) -> CacheResult<u64> {
        let key = AuthCache::new_key(mem_id);
        let res = self.chx.json_del::<AuthCache>(key.as_ref(), None).await?;

        Ok(res)
    }
}
