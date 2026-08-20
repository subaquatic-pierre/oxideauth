use std::sync::Arc;

use tracing::info;
use uuid::Uuid;

use crate::cache::{
    entities::workspace::WorkspaceCache,
    error::CacheResult,
    traits::{CacheEntity, CacheExecutor, CacheKey},
};

/// Store for cached workspace entities.
///
/// Each workspace is stored as a single JSON value keyed by its id. On cache
/// miss the caller hydrates the entity from the database and writes it back
/// through `write`; mutations invalidate via `invalidate`/`invalidate_by_id`.
pub struct WorkspaceCacheStore<C: CacheExecutor> {
    chx: Arc<C>,
}

impl<C: CacheExecutor> WorkspaceCacheStore<C> {
    pub fn new(chx: Arc<C>) -> Self {
        Self { chx }
    }

    /// Reads the workspace entity for the given key. Returns `None` on a miss.
    pub async fn fetch_by_id(&self, id: Uuid) -> CacheResult<Option<WorkspaceCache>> {
        let key = WorkspaceCache::new_key(id);
        self.chx.json_get(key.as_ref(), None).await
    }

    /// Reads the workspace entity for the given key. Returns `None` on a miss.
    pub async fn fetch(&self, key: &CacheKey) -> CacheResult<Option<WorkspaceCache>> {
        self.chx.json_get(key.as_ref(), None).await
    }

    /// Writes the workspace entity with the given TTL (seconds).
    pub async fn write(&self, entity: &WorkspaceCache, ttl: Option<i64>) -> CacheResult<()> {
        self.chx
            .json_set(entity.key().as_ref(), None, entity, ttl)
            .await?;
        info!(workspace_id = %entity.id, "WORKSPACE_CACHE_HYDRATED");
        Ok(())
    }

    /// Deletes the workspace entity. Used on invalidation.
    pub async fn invalidate(&self, ws_id: Uuid) -> CacheResult<i64> {
        let key = WorkspaceCache::new_key(ws_id);
        let t = self
            .chx
            .json_del::<WorkspaceCache>(key.as_ref(), None)
            .await?;

        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::mock::MockChx;

    fn sample_workspace() -> WorkspaceCache {
        WorkspaceCache {
            id: Uuid::new_v4(),
            name: "Acme".into(),
            slug: "acme".into(),
            description: Some("A test workspace".into()),
            owner: Uuid::new_v4(),
            config: Default::default(),
            tags: vec!["tag-a".into()],
            meta: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_write_then_fetch_roundtrip() {
        let store = WorkspaceCacheStore::new(Arc::new(MockChx::new()));
        let entity = sample_workspace();

        store.write(&entity, None).await.unwrap();

        let fetched = store
            .fetch(&entity.key())
            .await
            .unwrap()
            .expect("cache hit after write");
        assert_eq!(fetched.id, entity.id);
        assert_eq!(fetched.name, "Acme");
        assert_eq!(fetched.slug, "acme");
        assert_eq!(fetched.description.as_deref(), Some("A test workspace"));
        assert_eq!(fetched.owner, entity.owner);
        assert_eq!(fetched.tags, vec!["tag-a".to_string()]);
    }

    #[tokio::test]
    async fn test_write_then_fetch_by_id_roundtrip() {
        let store = WorkspaceCacheStore::new(Arc::new(MockChx::new()));
        let entity = sample_workspace();

        store.write(&entity, None).await.unwrap();

        let fetched = store
            .fetch_by_id(entity.id)
            .await
            .unwrap()
            .expect("cache hit by id");
        assert_eq!(fetched.id, entity.id);
        assert_eq!(fetched.slug, "acme");
    }

    #[tokio::test]
    async fn test_fetch_miss_returns_none() {
        let store = WorkspaceCacheStore::new(Arc::new(MockChx::new()));
        let fetched = store.fetch_by_id(Uuid::new_v4()).await.unwrap();
        assert!(fetched.is_none());
    }

    #[tokio::test]
    async fn test_invalidate_removes_entity() {
        let store = WorkspaceCacheStore::new(Arc::new(MockChx::new()));
        let entity = sample_workspace();

        store.write(&entity, None).await.unwrap();

        let removed = store.invalidate(entity.id).await.unwrap();
        assert_eq!(removed, 1, "invalidate should remove the stored entity");

        let fetched = store.fetch_by_id(entity.id).await.unwrap();
        assert!(fetched.is_none(), "fetch after invalidate must be a miss");
    }
}
