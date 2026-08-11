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
    pub system_acc_name: &'static str,
    pub system_acc_email: &'static str,
    pub workspace_header_key: &'static str,
}

pub const SYSTEM_CONST: SystemConstants = SystemConstants {
    system_ws_slug: "system",
    system_acc_name: "system",
    system_acc_email: "system@system.local",
    workspace_header_key: "X-Workspace-Id",
};

// -----------------------------------------------------------------------------
// endregion: --- Base Trait Implementations

// region:    --- Tests
// -----------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        dev::init::init_test,
        store::{
            ctx::StoreCtx,
            entities::{project::ProjectForCreate, workspace::WorkspaceForCreate},
            error::StoreError,
            traits::{contains::FilterByContains, crud::*, join::GetOneToMany},
        },
    };
    use anyhow::Result;
    use serde_json::json;
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_create_get_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = WorkspaceStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let data = WorkspaceForCreate {
            name: "test-workspace-create".to_string(),
            ..Default::default()
        };

        // -- Execute
        let created_workspace = store.create(&ctx, data).await?;
        let fetched_workspace = store.get(&ctx, &created_workspace.id).await?;

        // -- Assert
        assert_eq!(created_workspace.name, "test-workspace-create");
        assert_eq!(fetched_workspace.id, created_workspace.id);
        assert_eq!(fetched_workspace.name, created_workspace.name);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_update_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = WorkspaceStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let created_workspace = store.create(&ctx, WorkspaceForCreate::default()).await?;

        let update_data = WorkspaceForUpdate {
            name: Some("updated-workspace-name".to_string()),
            ..Default::default()
        };

        // -- Execute
        let updated_workspace = store
            .update(&ctx, &created_workspace.id, update_data)
            .await?;
        let fetched_workspace = store.get(&ctx, &created_workspace.id).await?;

        // -- Assert
        assert_eq!(updated_workspace.name, "updated-workspace-name");
        assert_eq!(fetched_workspace.name, "updated-workspace-name");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = WorkspaceStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let created_workspace = store.create(&ctx, WorkspaceForCreate::default()).await?;

        // -- Execute
        let deleted_workspace = store.delete(&ctx, &created_workspace.id).await?;
        let get_result = store.get(&ctx, &created_workspace.id).await;

        // -- Assert
        assert_eq!(deleted_workspace.id, created_workspace.id);
        assert!(
            matches!(get_result, Err(StoreError::EntityNotFound { .. })),
            "Getting the workspace after deletion should fail"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_list_with_filter_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = WorkspaceStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let ns_to_create = vec![
            WorkspaceForCreate {
                name: "list-ns-XY".to_string(),
                ..Default::default()
            },
            WorkspaceForCreate {
                name: "list-ns-ZB".to_string(),
                ..Default::default()
            },
        ];
        store.create_many(&ctx, ns_to_create).await?;

        // -- Execute
        let filter: WorkspaceFilter = json!({ "name": "list-ns-XY" }).try_into()?;
        let workspaces = store.list(&ctx, Some(filter), None).await?;

        // -- Assert
        assert_eq!(workspaces.len(), 1);
        assert_eq!(workspaces[0].name, "list-ns-XY");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_get_one_to_many_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = WorkspaceStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let workspace = store
            .create(
                &ctx,
                WorkspaceForCreate {
                    name: "one-to-many-ns".into(),
                    ..Default::default()
                },
            )
            .await?;

        // Manually insert related projects
        app.sm
            .project
            .create(
                &ctx,
                ProjectForCreate {
                    workspace_id: workspace.id.into(),
                    name: "project-a".into(),
                    ..Default::default()
                },
            )
            .await?;
        app.sm
            .project
            .create(
                &ctx,
                ProjectForCreate {
                    workspace_id: workspace.id.into(),
                    name: "project-b".into(),
                    ..Default::default()
                },
            )
            .await?;

        // -- Execute
        let ns_with_projects = store.get_one_to_many(&ctx, &workspace.id).await?;

        // -- Assert
        assert_eq!(ns_with_projects.id, workspace.id);
        assert_eq!(
            ns_with_projects.projects.len(),
            2,
            "Should have 2 projects attached"
        );

        let has_project_a = ns_with_projects
            .projects
            .iter()
            .any(|p| p.name == "project-a");
        let has_project_b = ns_with_projects
            .projects
            .iter()
            .any(|p| p.name == "project-b");
        assert!(has_project_a, "Should contain project-a");
        assert!(has_project_b, "Should contain project-b");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_filter_by_contains_tags() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = WorkspaceStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Create test data with different tags
        store
            .create(
                &ctx,
                WorkspaceForCreate {
                    name: "tags-ns-a".into(),
                    tags: vec!["org".into(), "production".into()],
                    ..Default::default()
                },
            )
            .await?;
        store
            .create(
                &ctx,
                WorkspaceForCreate {
                    name: "tags-ns-b".into(),
                    tags: vec!["user".into(), "personal".into()],
                    ..Default::default()
                },
            )
            .await?;

        // -- Execute & Assert
        let org_workspaces = store
            .filter_by_tags_contain(&ctx, vec!["org".into()], None)
            .await?;
        assert_eq!(
            org_workspaces.len(),
            1,
            "Should find 1 workspace with 'org' tag"
        );
        assert_eq!(org_workspaces[0].name, "tags-ns-a");

        let personal_workspaces = store
            .filter_by_tags_contain(&ctx, vec!["personal".into()], None)
            .await?;
        assert_eq!(
            personal_workspaces.len(),
            1,
            "Should find 1 workspace with 'personal' tag"
        );
        assert_eq!(personal_workspaces[0].name, "tags-ns-b");

        Ok(())
    }
}
// -----------------------------------------------------------------------------
// endregion: --- Tests
