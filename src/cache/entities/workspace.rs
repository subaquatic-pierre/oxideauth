use std::{fmt::Display, sync::Arc};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    cache::{
        error::CacheResult,
        traits::{CacheEntity, CacheKey},
    },
    core::models::workspace::Workspace,
    store::{
        crud::Get,
        ctx::StoreCtx,
        entities::workspace::{WorkspaceConfig, WorkspaceMeta, WorkspaceRow},
        manager::StoreManager,
        stores::workspace::SYSTEM_CONST,
        traits::dbx::DbExecutor,
    },
};

/// The cached workspace payload persisted under `oxauth:ws:{workspace_id}`.
///
/// Carries the workspace identity and configuration needed to reconstruct a
/// [`Workspace`] without hitting the database on every request. `id` is the
/// identifier fixed at construction time and used to compute the Redis key;
/// the remaining fields are cached values populated after a `fetch` (cache hit)
/// or hydration (cache miss).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceCache {
    // Identifier (set at construction)
    pub id: Uuid,

    // Identity
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub owner: Uuid,

    // Config
    pub config: WorkspaceConfig,

    pub tags: Vec<String>,
    pub meta: WorkspaceMeta,
}

impl WorkspaceCache {
    /// Creates a keyed template with the identifier set and values at defaults.
    /// Used as the template passed to `WorkspaceCacheStore::fetch`.
    pub fn new_keyed(id: Uuid) -> Self {
        Self {
            id,
            name: String::new(),
            slug: String::new(),
            description: None,
            owner: Uuid::nil(),
            config: WorkspaceConfig::default(),
            tags: vec![],
            meta: WorkspaceMeta::default(),
        }
    }

    pub fn bootstrap() -> Self {
        Self {
            id: Uuid::nil(),
            name: String::new(),
            slug: SYSTEM_CONST.system_ws_slug.to_string(),
            description: None,
            owner: Uuid::nil(),
            config: WorkspaceConfig::default(),
            tags: vec![],
            meta: WorkspaceMeta::default(),
        }
    }

    pub async fn build_from_db<D: DbExecutor>(
        sm: Arc<StoreManager<D>>,
        ws_id: Uuid,
    ) -> CacheResult<WorkspaceCache> {
        let store_ctx = StoreCtx::bootstrap();
        let workspace_row = sm.workspace.get(&store_ctx, &ws_id.into()).await?;

        Ok(WorkspaceCache::from(workspace_row))
    }
}

impl Default for WorkspaceCache {
    fn default() -> Self {
        Self::new_keyed(Uuid::nil())
    }
}

impl From<WorkspaceRow> for WorkspaceCache {
    fn from(row: WorkspaceRow) -> Self {
        Self {
            id: row.id.into(),
            name: row.name,
            slug: row.slug,
            description: row.description,
            owner: row.owner.into(),
            config: row.config,
            tags: row.tags,
            meta: row.meta,
        }
    }
}

impl From<Workspace> for WorkspaceCache {
    fn from(row: Workspace) -> Self {
        Self {
            id: row.id.into(),
            name: row.name,
            slug: row.slug,
            description: row.description,
            owner: row.owner.into(),
            config: row.config,
            tags: row.tags,
            meta: row.meta,
        }
    }
}

impl CacheEntity for WorkspaceCache {
    fn _key() -> (&'static str, &'static str) {
        ("oxauth", "ws")
    }

    fn key(&self) -> CacheKey {
        let (prefix, name) = Self::_key();
        CacheKey::new(prefix, name, self.id)
    }

    fn new_key(id: impl Display) -> CacheKey {
        let (prefix, name) = Self::_key();
        CacheKey::new(prefix, name, id)
    }
}
