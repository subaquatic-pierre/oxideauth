use std::sync::Arc;

use sea_query::Iden;
use serde_json::json;

use crate::store::{
    crud::List,
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::{
        id::DbId,
        permission::{
            PermissionFilter, PermissionForCreate, PermissionForUpdate, PermissionIden,
            PermissionRow,
        },
    },
    error::{StoreError, StoreResult},
    queries::{
        batch::find_many_where_value_in_key,
        meta::{ContainsFilterQueryMeta, MutateQueryMeta, ReadQueryMeta},
    },
    traits::{
        dbx::DbExecutor,
        meta::{ContainsFilterStore, MutateStore, ReadStore, Store},
    },
};

/// The struct for our Permission store, holding the database connection wrapper.
pub struct PermissionStore<D: DbExecutor> {
    dbx: Arc<D>,
    has_audit: bool,
}

impl<D: DbExecutor> PermissionStore<D> {
    /// Creates a new `PermissionStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        Self {
            dbx,
            has_audit: true,
        }
    }

    pub async fn find_all_many_by_names(
        &self,
        ctx: &StoreCtx,
        names: Vec<String>,
    ) -> StoreResult<Vec<PermissionRow>> {
        let meta = ReadQueryMeta {
            table: PermissionIden::Table,
            pk: PermissionIden::Name,
            has_audit: self.has_audit,
        };

        let res = find_many_where_value_in_key(ctx, &self.dbx, names, &meta).await?;

        Ok(res)
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, PermissionStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for PermissionStore<D> {
    type Iden = PermissionIden;
    type Row = PermissionRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for PermissionStore<D> {
    type FilterStoreParams = PermissionFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: PermissionIden::Table,
            pk: PermissionIden::Id,
            has_audit: self.has_audit,
        }
    }
}

impl<D: DbExecutor> MutateStore for PermissionStore<D> {
    type CreateStoreParams = PermissionForCreate;
    type UpdateStoreParams = PermissionForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: PermissionIden::Table,
            pk: PermissionIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for PermissionStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: PermissionIden::Table,
            col: PermissionIden::Tags,
            has_audit: true,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: PermissionIden::Table,
            col: PermissionIden::Meta,
            has_audit: true,
        }
    }
}

// -----------------------------------------------------------------------------
// endregion: --- Base Trait Implementations

// region:    --- Tests
// -----------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cache::redis::RedisChx,
        dev::init::init_test,
        store::{
            ctx::StoreCtx,
            entities::{permission::PermissionForCreate, workspace::WorkspaceForCreate},
            error::StoreError,
            traits::{contains::FilterByContains, crud::*},
        },
    };
    use anyhow::Result;
    use serde_json::json;
    use serial_test::serial;
    use uuid::Uuid;

    /// Helper function to seed the necessary Workspace for a Permission.
    async fn seed_prerequisite(
        ctx: &StoreCtx,
        app: &crate::app::AppState<PgDbx, RedisChx>,
    ) -> Result<Uuid> {
        let workspace = app
            .sm
            .workspace
            .create(ctx, WorkspaceForCreate::default())
            .await?;
        Ok(workspace.id.into())
    }

    #[tokio::test]
    #[serial]
    async fn test_create_get_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = PermissionStore::new(dbx);
        let ctx = StoreCtx::bootstrap();
        let workspace_id = seed_prerequisite(&ctx, &app).await?;

        let data = PermissionForCreate {
            name: "project:create".to_string(),
            workspace_id,
            ..Default::default()
        };

        // -- Execute
        let created_permission = store.create(&ctx, data).await?;
        let fetched_permission = store.get(&ctx, &created_permission.id).await?;

        // -- Assert
        assert_eq!(created_permission.name, "project:create");
        assert_eq!(created_permission.workspace_id, workspace_id);
        assert_eq!(fetched_permission.id, created_permission.id);
        assert_eq!(fetched_permission.name, created_permission.name);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_update_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = PermissionStore::new(dbx);
        let ctx = StoreCtx::bootstrap();
        let workspace_id = seed_prerequisite(&ctx, &app).await?;

        let created_permission = store
            .create(
                &ctx,
                PermissionForCreate {
                    workspace_id,
                    ..Default::default()
                },
            )
            .await?;

        let update_data = PermissionForUpdate {
            name: Some("user:create".into()),
            ..Default::default()
        };

        // -- Execute
        let updated_permission = store
            .update(&ctx, &created_permission.id, update_data)
            .await?;
        let fetched_permission = store.get(&ctx, &created_permission.id).await?;

        // -- Assert
        assert_eq!(updated_permission.name, "user:create".to_string());
        assert_eq!(fetched_permission.name, "user:create".to_string());

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = PermissionStore::new(dbx);
        let ctx = StoreCtx::bootstrap();
        let workspace_id = seed_prerequisite(&ctx, &app).await?;

        let created_permission = store
            .create(
                &ctx,
                PermissionForCreate {
                    workspace_id,
                    ..Default::default()
                },
            )
            .await?;

        // -- Execute
        let deleted_permission = store.delete(&ctx, &created_permission.id).await?;
        let get_result = store.get(&ctx, &created_permission.id).await;

        // -- Assert
        assert_eq!(deleted_permission.id, created_permission.id);
        assert!(
            matches!(get_result, Err(StoreError::EntityNotFound { .. })),
            "Getting the permission after deletion should fail"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_list_with_filter_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = PermissionStore::new(dbx);
        let ctx = StoreCtx::bootstrap();
        let workspace_id = seed_prerequisite(&ctx, &app).await?;

        let perms_to_create = vec![
            PermissionForCreate {
                name: "perm:list:a".to_string(),
                workspace_id,
                ..Default::default()
            },
            PermissionForCreate {
                name: "perm:list:b".to_string(),
                workspace_id,
                ..Default::default()
            },
        ];
        store.create_many(&ctx, perms_to_create).await?;

        // -- Execute
        let filter: PermissionFilter = json!({ "name": "perm:list:b" }).try_into()?;
        let permissions = store.list(&ctx, Some(filter), None).await?;

        // -- Assert
        assert_eq!(permissions.len(), 1);
        assert_eq!(permissions[0].name, "perm:list:b");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_filter_by_contains_tags() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = PermissionStore::new(dbx);
        let ctx = StoreCtx::bootstrap();
        let workspace_id = seed_prerequisite(&ctx, &app).await?;

        // -- Create test data with different tags
        store
            .create(
                &ctx,
                PermissionForCreate {
                    name: "tags-perm-a".into(),
                    tags: vec!["resource".into(), "project".into()],
                    workspace_id,
                    ..Default::default()
                },
            )
            .await?;
        store
            .create(
                &ctx,
                PermissionForCreate {
                    name: "tags-perm-b".into(),
                    tags: vec!["action".into(), "delete".into()],
                    workspace_id,
                    ..Default::default()
                },
            )
            .await?;

        // -- Execute & Assert
        let project_perms = store
            .filter_by_tags_contain(&ctx, vec!["project".into()], None)
            .await?;
        assert_eq!(
            project_perms.len(),
            1,
            "Should find 1 permission with 'project' tag"
        );
        assert_eq!(project_perms[0].name, "tags-perm-a");

        let delete_perms = store
            .filter_by_tags_contain(&ctx, vec!["delete".into()], None)
            .await?;
        assert_eq!(
            delete_perms.len(),
            1,
            "Should find 1 permission with 'delete' tag"
        );
        assert_eq!(delete_perms[0].name, "tags-perm-b");

        Ok(())
    }
}
// -----------------------------------------------------------------------------
// endregion: --- Tests
