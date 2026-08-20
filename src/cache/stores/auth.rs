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
    pub async fn write(&self, entity: &AuthCache, ttl: Option<i64>) -> CacheResult<()> {
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
    pub async fn invalidate(&self, mem_id: Uuid) -> CacheResult<i64> {
        let key = AuthCache::new_key(mem_id);
        let res = self.chx.json_del::<AuthCache>(key.as_ref(), None).await?;

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::mock::MockChx;

    #[tokio::test]
    async fn test_write_then_fetch_roundtrip() {
        let store = AuthCacheStore::new(Arc::new(MockChx::new()));

        let entity = AuthCache::new_keyed(Uuid::new_v4(), Uuid::new_v4(), Some(Uuid::new_v4()));
        store.write(&entity, None).await.unwrap();

        let fetched = store
            .fetch(&entity.key())
            .await
            .unwrap()
            .expect("cache hit after write");
        assert_eq!(fetched.mem_id, entity.mem_id);
        assert_eq!(fetched.acc_id, entity.acc_id);
        assert_eq!(fetched.sid, entity.sid);
        assert_eq!(fetched.mem_version, entity.mem_version);
        assert_eq!(fetched.acc_version, entity.acc_version);
    }

    #[tokio::test]
    async fn test_fetch_miss_returns_none() {
        let store = AuthCacheStore::new(Arc::new(MockChx::new()));
        let mem = Uuid::new_v4();
        let fetched = store.fetch(&AuthCache::new_key(mem)).await.unwrap();
        assert!(fetched.is_none());
    }

    #[tokio::test]
    async fn test_invalidate_removes_entity() {
        let store = AuthCacheStore::new(Arc::new(MockChx::new()));

        let mem = Uuid::new_v4();
        let entity = AuthCache::new_keyed(mem, Uuid::new_v4(), None);
        store.write(&entity, None).await.unwrap();

        let removed = store.invalidate(mem).await.unwrap();
        assert_eq!(removed, 1, "invalidate should remove the stored entity");

        let fetched = store.fetch(&entity.key()).await.unwrap();
        assert!(fetched.is_none(), "fetch after invalidate must be a miss");
    }
}
