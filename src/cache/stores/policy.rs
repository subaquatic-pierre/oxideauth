//! Cache stores for the policy engine.

use std::sync::Arc;

use tracing::info;
use uuid::Uuid;

use crate::cache::{
    entities::policy::PolicyCache,
    error::CacheResult,
    traits::{CacheEntity, CacheExecutor, CacheKey},
};

/// Store for cached per-membership policy sets.
///
/// Each membership's compiled [`PolicySet`] is stored as a single JSON value
/// keyed by its membership id (`oxauth:policy:{mem_id}`). On cache miss the
/// caller hydrates the set from the database (`PolicyService::resolve_for_membership`)
/// and writes it back through `write`; mutations invalidate via `invalidate`.
pub struct PolicyCacheStore<C: CacheExecutor> {
    chx: Arc<C>,
}

impl<C: CacheExecutor> PolicyCacheStore<C> {
    pub fn new(chx: Arc<C>) -> Self {
        Self { chx }
    }

    /// Reads the policy entity for the given key. Returns `None` on a miss.
    pub async fn fetch(&self, key: &CacheKey) -> CacheResult<Option<PolicyCache>> {
        self.chx.json_get(key.as_ref(), None).await
    }

    /// Writes the policy entity with the given TTL (seconds).
    pub async fn write(&self, entity: &PolicyCache, ttl: Option<i64>) -> CacheResult<()> {
        self.chx
            .json_set(entity.key().as_ref(), None, entity, ttl)
            .await?;
        info!(membership_id = %entity.mem_id, "POLICY_CACHE_HYDRATED");
        Ok(())
    }

    /// Deletes the policy entity for a membership. Used on invalidation.
    pub async fn invalidate(&self, mem_id: Uuid) -> CacheResult<i64> {
        let key = PolicyCache::new_key(mem_id);
        let t = self.chx.json_del::<PolicyCache>(key.as_ref(), None).await?;

        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::mock::MockChx;
    use crate::core::models::policy::{Policy, PolicyEffect, PolicySet};

    fn sample_cache(mem_id: Uuid) -> PolicyCache {
        let mut p = Policy::default();
        p.id = Uuid::new_v4();
        p.actions = vec![
            "membership:update".to_string(),
            "profile:update".to_string(),
        ];
        p.resource = "self".to_string();
        let set = PolicySet::from_policies(vec![p]);
        PolicyCache::new(mem_id, set)
    }

    #[tokio::test]
    async fn test_write_then_fetch_roundtrip() {
        let store = PolicyCacheStore::new(Arc::new(MockChx::new()));
        let entity = sample_cache(Uuid::new_v4());

        store.write(&entity, None).await.unwrap();

        let fetched = store
            .fetch(&entity.key())
            .await
            .unwrap()
            .expect("cache hit after write");
        assert_eq!(fetched.mem_id, entity.mem_id);
        assert_eq!(fetched.policies, entity.policies);
        assert_eq!(
            fetched.policies.get("membership:update", "self", None),
            Some(PolicyEffect::Allow)
        );
    }

    #[tokio::test]
    async fn test_fetch_miss_returns_none() {
        let store = PolicyCacheStore::new(Arc::new(MockChx::new()));
        let mem_id = Uuid::new_v4();
        let fetched = store.fetch(&PolicyCache::new_key(mem_id)).await.unwrap();
        assert!(fetched.is_none());
    }

    #[tokio::test]
    async fn test_invalidate_removes_entity() {
        let store = PolicyCacheStore::new(Arc::new(MockChx::new()));
        let mem_id = Uuid::new_v4();
        let entity = sample_cache(mem_id);

        store.write(&entity, None).await.unwrap();

        let removed = store.invalidate(mem_id).await.unwrap();
        assert_eq!(removed, 1, "invalidate should remove the stored entity");

        let fetched = store.fetch(&entity.key()).await.unwrap();
        assert!(fetched.is_none(), "fetch after invalidate must be a miss");
    }

    #[tokio::test]
    async fn test_invalidate_absent_entity_returns_zero() {
        let store = PolicyCacheStore::new(Arc::new(MockChx::new()));
        let removed = store.invalidate(Uuid::new_v4()).await.unwrap();
        assert_eq!(removed, 0, "invalidate of a missing entity returns 0");
    }
}
