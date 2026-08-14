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
    use crate::store::{
        ctx::StoreCtx,
        dbx::MockDbx,
        entities::{audit::AuditFields, permission::PermissionMeta},
        error::StoreError,
        traits::{contains::FilterByContains, crud::*},
    };
    use anyhow::Result;
    use serde_json::json;
    use uuid::Uuid;

    /// Helper to build a `PermissionRow` with default-ish values for the mock.
    fn permission_row() -> PermissionRow {
        PermissionRow {
            id: DbId::from(Uuid::new_v4()),
            workspace_id: Uuid::new_v4(),
            name: String::new(),
            description: None,
            tags: vec![],
            meta: PermissionMeta::default(),
            audit: AuditFields::default(),
        }
    }

    #[tokio::test]
    async fn test_create_get_ok() -> Result<()> {
        // -- Setup
        let workspace_id = Uuid::new_v4();
        let permission_id = DbId::from(Uuid::new_v4());
        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<PermissionRow>(PermissionRow {
                    id: permission_id,
                    name: "project:create".into(),
                    workspace_id,
                    ..permission_row()
                })
                .with_optional::<PermissionRow>(Some(PermissionRow {
                    id: permission_id,
                    name: "project:create".into(),
                    ..permission_row()
                })),
        );
        let store = PermissionStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

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
    async fn test_update_ok() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<PermissionRow>(permission_row())
                .with_optional::<PermissionRow>(Some(PermissionRow {
                    name: "user:create".into(),
                    ..permission_row()
                }))
                .with_optional::<PermissionRow>(Some(PermissionRow {
                    name: "user:create".into(),
                    ..permission_row()
                })),
        );
        let store = PermissionStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let created_permission = store.create(&ctx, PermissionForCreate::default()).await?;
        let updated_permission = store
            .update(
                &ctx,
                &created_permission.id,
                PermissionForUpdate {
                    name: Some("user:create".into()),
                    ..Default::default()
                },
            )
            .await?;
        let fetched_permission = store.get(&ctx, &created_permission.id).await?;

        // -- Assert
        assert_eq!(updated_permission.name, "user:create".to_string());
        assert_eq!(fetched_permission.name, "user:create".to_string());

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_ok() -> Result<()> {
        // -- Setup
        let permission_id = DbId::from(Uuid::new_v4());
        let dbx = Arc::new(
            MockDbx::new()
                .with_optional::<PermissionRow>(Some(PermissionRow {
                    id: permission_id,
                    ..permission_row()
                }))
                .with_optional::<PermissionRow>(None),
        );
        let store = PermissionStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let deleted_permission = store.delete(&ctx, &permission_id).await?;
        let get_result = store.get(&ctx, &permission_id).await;

        // -- Assert
        assert_eq!(deleted_permission.id, permission_id);
        assert!(
            matches!(get_result, Err(StoreError::EntityNotFound { .. })),
            "Getting the permission after deletion should fail"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_list_with_filter_ok() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<PermissionRow>(vec![permission_row(), permission_row()])
                .with_all::<PermissionRow>(vec![PermissionRow {
                    name: "perm:list:b".into(),
                    ..permission_row()
                }]),
        );
        let store = PermissionStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        store
            .create_many(
                &ctx,
                vec![PermissionForCreate::default(), PermissionForCreate::default()],
            )
            .await?;

        let filter: PermissionFilter = json!({ "name": "perm:list:b" }).try_into()?;
        let permissions = store.list(&ctx, Some(filter), None).await?;

        // -- Assert
        assert_eq!(permissions.len(), 1);
        assert_eq!(permissions[0].name, "perm:list:b");

        Ok(())
    }

    #[tokio::test]
    async fn test_filter_by_contains_tags() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<PermissionRow>(vec![PermissionRow {
                    name: "tags-perm-a".into(),
                    tags: vec!["resource".into(), "project".into()],
                    ..permission_row()
                }])
                .with_all::<PermissionRow>(vec![PermissionRow {
                    name: "tags-perm-b".into(),
                    tags: vec!["action".into(), "delete".into()],
                    ..permission_row()
                }]),
        );
        let store = PermissionStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

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
