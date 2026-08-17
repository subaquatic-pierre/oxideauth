use std::sync::Arc;

use sea_query::Iden;
use serde_json::json;

use crate::store::{
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::workspace::{
        WorkspaceFilter, WorkspaceForCreate, WorkspaceForUpdate, WorkspaceIden, WorkspaceRow,
        WorkspaceWithProjects,
    },
    error::{StoreError, StoreResult},
    queries::meta::{ContainsFilterQueryMeta, MutateQueryMeta, OneToManyQueryMeta, ReadQueryMeta},
    traits::{
        crud::List,
        dbx::DbExecutor,
        meta::{ContainsFilterStore, MutateStore, OneToManyStore, ReadStore, Store},
    },
};

/// The struct for our Workspace store, holding the database connection wrapper.
pub struct WorkspaceStore<D: DbExecutor> {
    dbx: Arc<D>,
}

impl<D: DbExecutor> WorkspaceStore<D> {
    /// Creates a new `WorkspaceStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        // Use generic
        Self { dbx }
    }

    pub async fn get_system_ws(&self, ctx: &StoreCtx) -> StoreResult<WorkspaceRow> {
        let ws = self
            .get_by_slug_opt(ctx, SYSTEM_CONST.system_ws_slug)
            .await?
            .ok_or_else(|| StoreError::EntityNotFound {
                entity: "workspace".to_string(),
                id: SYSTEM_CONST.system_ws_slug.to_string(),
            })?;
        Ok(ws)
    }

    pub async fn get_by_slug(&self, ctx: &StoreCtx, slug: &str) -> StoreResult<WorkspaceRow> {
        match self.get_by_slug_opt(ctx, slug).await? {
            Some(row) => Ok(row),
            None => Err(StoreError::EntityNotFound {
                entity: self.read_meta().table.to_string(),
                id: slug.to_string(),
            }),
        }
    }

    pub async fn get_by_slug_opt(
        &self,
        ctx: &StoreCtx,
        slug: &str,
    ) -> StoreResult<Option<WorkspaceRow>> {
        let filter: WorkspaceFilter = json!({
            "slug": slug.to_string()
        })
        .try_into()?;

        Ok(self.list(ctx, Some(filter), None).await?.into_iter().next())
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, WorkspaceStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for WorkspaceStore<D> {
    type Iden = WorkspaceIden;
    type Row = WorkspaceRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for WorkspaceStore<D> {
    type FilterStoreParams = WorkspaceFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: WorkspaceIden::Table,
            pk: WorkspaceIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> MutateStore for WorkspaceStore<D> {
    type CreateStoreParams = WorkspaceForCreate;
    type UpdateStoreParams = WorkspaceForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: WorkspaceIden::Table,
            pk: WorkspaceIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> OneToManyStore for WorkspaceStore<D> {
    type OneToManyRow = WorkspaceWithProjects;

    type FilterStoreParams = WorkspaceFilter;

    fn one_to_many_meta(&self) -> OneToManyQueryMeta<Self::Iden> {
        OneToManyQueryMeta {
            single_table: WorkspaceIden::Table,
            many_table: WorkspaceIden::Project,
            single_pk: WorkspaceIden::Id,
            many_pk: WorkspaceIden::Id,
            many_fk: WorkspaceIden::WorkspaceId,
            agg_alias: WorkspaceIden::Projects,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for WorkspaceStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: WorkspaceIden::Table,
            col: WorkspaceIden::Tags,
            has_audit: true,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: WorkspaceIden::Table,
            col: WorkspaceIden::Meta,
            has_audit: true,
        }
    }
}

pub struct SystemConstants {
    pub system_ws_slug: &'static str,
    pub default_ws_slug: &'static str,
    pub system_acc_name: &'static str,
    pub system_acc_email: &'static str,
    pub workspace_header_key: &'static str,
    pub workspace_viewer_role: &'static str,
    pub workspace_admin_role: &'static str,
}

pub const SYSTEM_CONST: SystemConstants = SystemConstants {
    system_ws_slug: "system",
    default_ws_slug: "default",
    system_acc_name: "system",
    system_acc_email: "system@system.local",
    workspace_header_key: "X-Workspace-Id",
    workspace_viewer_role: "WorkspaceViewer",
    workspace_admin_role: "WorkspaceAdmin",
};

// -----------------------------------------------------------------------------
// endregion: --- Base Trait Implementations

// region:    --- Tests
// -----------------------------------------------------------------------------
