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
    pub async fn write(&self, entity: &WorkspaceCache, ttl: Option<u64>) -> CacheResult<()> {
        self.chx
            .json_set(entity.key().as_ref(), None, entity, ttl)
            .await?;
        info!(workspace_id = %entity.id, "WORKSPACE_CACHE_HYDRATED");
        Ok(())
    }

    /// Deletes the workspace entity. Used on invalidation.
    pub async fn invalidate(&self, ws_id: Uuid) -> CacheResult<u64> {
        let key = WorkspaceCache::new_key(ws_id);
        let t = self
            .chx
            .json_del::<WorkspaceCache>(key.as_ref(), None)
            .await?;

        Ok(t)
    }
}
