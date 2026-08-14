use std::sync::Arc;

use modql::filter::ListOptions;
use sea_query::Iden;
use serde_json::json;

use crate::store::{
    crud::List,
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::{
        id::DbId,
        role::{RoleFilter, RoleForCreate, RoleForUpdate, RoleIden, RoleRow, RoleWithPermissions},
    },
    error::{StoreError, StoreResult},
    queries::{
        list::list_containing_many,
        meta::{
            ContainsFilterQueryMeta, ListContainingManyQueryMeta, ManyToManyQueryMeta,
            MutateQueryMeta, ReadQueryMeta,
        },
    },
    traits::{
        dbx::DbExecutor,
        meta::{ContainsFilterStore, ManyToManyStore, MutateStore, ReadStore, Store},
    },
};
/// The struct for our Role store, holding the database connection wrapper.
pub struct RoleStore<D: DbExecutor> {
    dbx: Arc<D>,
}

impl<D: DbExecutor> RoleStore<D> {
    /// Creates a new `RoleStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        Self { dbx }
    }

    pub async fn get_by_name(
        &self,
        ctx: &StoreCtx,
        name: &str,
        workspace_id: DbId,
    ) -> StoreResult<RoleRow> {
        match self.get_by_name_opt(ctx, name, workspace_id).await? {
            Some(row) => Ok(row),
            None => Err(StoreError::EntityNotFound {
                entity: self.read_meta().table.to_string(),
                id: name.to_string(),
            }),
        }
    }

    pub async fn get_by_name_opt(
        &self,
        ctx: &StoreCtx,
        name: &str,
        workspace_id: DbId,
    ) -> StoreResult<Option<RoleRow>> {
        let filter: RoleFilter = json!({
            "name": name.to_string(),
            "workspace_id": workspace_id.to_string()
        })
        .try_into()?;

        Ok(self.list(ctx, Some(filter), None).await?.into_iter().next())
    }

    /// Lists roles whose set of linked permissions **contains all** of the given
    /// permission IDs (via the `role_permission` join table).
    pub async fn list_containing_permissions(
        &self,
        ctx: &StoreCtx,
        permission_ids: Vec<DbId>,
        tags: Option<Vec<String>>,
        filter: Option<RoleFilter>,
        opts: Option<ListOptions>,
    ) -> StoreResult<Vec<RoleRow>> {
        let meta = ListContainingManyQueryMeta {
            table: RoleIden::Table,
            pk: RoleIden::Id,
            join_table: RoleIden::RolePermission,
            join_fk: RoleIden::RoleId,
            join_many_fk: RoleIden::PermissionId,
            has_audit: true,
        };

        list_containing_many(ctx, &self.dbx, permission_ids, tags, filter, opts, &meta).await
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, RoleStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for RoleStore<D> {
    type Iden = RoleIden;
    type Row = RoleRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for RoleStore<D> {
    type FilterStoreParams = RoleFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: RoleIden::Table,
            pk: RoleIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> MutateStore for RoleStore<D> {
    type CreateStoreParams = RoleForCreate;
    type UpdateStoreParams = RoleForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: RoleIden::Table,
            pk: RoleIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ManyToManyStore for RoleStore<D> {
    type ManyToManyRow = RoleWithPermissions;

    type FilterStoreParams = RoleFilter;

    fn many_to_many_meta(&self) -> ManyToManyQueryMeta<Self::Iden> {
        ManyToManyQueryMeta {
            single_table: RoleIden::Table,
            many_table: RoleIden::Permission,
            join_table: RoleIden::RolePermission,
            single_pk: RoleIden::Id,
            many_pk: RoleIden::PermissionPk,
            many_fk: RoleIden::PermissionId,
            join_fk: RoleIden::RoleId,
            agg_alias: RoleIden::Permissions,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for RoleStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: RoleIden::Table,
            col: RoleIden::Tags,
            has_audit: true,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: RoleIden::Table,
            col: RoleIden::Meta,
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
        entities::{
            audit::AuditFields,
            permission::PermissionMeta,
            role::{JoinedPermissionOnRole, RoleMeta},
        },
        error::StoreError,
        traits::{
            contains::FilterByContains,
            crud::*,
            join::GetManyToMany,
        },
    };
    use anyhow::Result;
    use serde_json::json;
    use time::OffsetDateTime;
    use uuid::Uuid;

    /// Helper to build a `RoleRow` with default-ish values for the mock.
    fn role_row() -> RoleRow {
        RoleRow {
            id: DbId::from(Uuid::new_v4()),
            workspace_id: Uuid::new_v4(),
            name: String::new(),
            description: None,
            tags: vec![],
            meta: RoleMeta::default(),
            audit: AuditFields::default(),
        }
    }

    #[tokio::test]
    async fn test_create_get_ok() -> Result<()> {
        // -- Setup
        let workspace_id = Uuid::new_v4();
        let role_id = DbId::from(Uuid::new_v4());
        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<RoleRow>(RoleRow {
                    id: role_id,
                    workspace_id,
                    name: "test-role-create".into(),
                    ..role_row()
                })
                .with_optional::<RoleRow>(Some(RoleRow {
                    id: role_id,
                    name: "test-role-create".into(),
                    ..role_row()
                })),
        );
        let store = RoleStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let data = RoleForCreate {
            workspace_id,
            name: "test-role-create".to_string(),
            ..Default::default()
        };

        // -- Execute
        let created_role = store.create(&ctx, data).await?;
        let fetched_role = store.get(&ctx, &created_role.id).await?;

        // -- Assert
        assert_eq!(created_role.name, "test-role-create");
        assert_eq!(created_role.workspace_id, workspace_id);
        assert_eq!(fetched_role.id, created_role.id);
        assert_eq!(fetched_role.name, created_role.name);

        Ok(())
    }

    #[tokio::test]
    async fn test_update_ok() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<RoleRow>(role_row())
                .with_optional::<RoleRow>(Some(RoleRow {
                    name: "admin".into(),
                    ..role_row()
                }))
                .with_optional::<RoleRow>(Some(RoleRow {
                    name: "admin".into(),
                    ..role_row()
                })),
        );
        let store = RoleStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let created_role = store.create(&ctx, RoleForCreate::default()).await?;
        let updated_role = store
            .update(
                &ctx,
                &created_role.id,
                RoleForUpdate {
                    name: Some("admin".to_string()),
                    ..Default::default()
                },
            )
            .await?;
        let fetched_role = store.get(&ctx, &created_role.id).await?;

        // -- Assert
        assert_eq!(updated_role.name, "admin".to_string());
        assert_eq!(fetched_role.name, "admin".to_string());

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_ok() -> Result<()> {
        // -- Setup
        let role_id = DbId::from(Uuid::new_v4());
        let dbx = Arc::new(
            MockDbx::new()
                .with_optional::<RoleRow>(Some(RoleRow {
                    id: role_id,
                    ..role_row()
                }))
                .with_optional::<RoleRow>(None),
        );
        let store = RoleStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let deleted_role = store.delete(&ctx, &role_id).await?;
        let get_result = store.get(&ctx, &role_id).await;

        // -- Assert
        assert_eq!(deleted_role.id, role_id);
        assert!(
            matches!(get_result, Err(StoreError::EntityNotFound { .. })),
            "Getting the role after deletion should fail"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_list_with_filter_ok() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<RoleRow>(vec![role_row(), role_row()])
                .with_all::<RoleRow>(vec![RoleRow {
                    name: "list-role-b".into(),
                    ..role_row()
                }]),
        );
        let store = RoleStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        store
            .create_many(&ctx, vec![RoleForCreate::default(), RoleForCreate::default()])
            .await?;

        let filter: RoleFilter = json!({ "name": "list-role-b" }).try_into()?;
        let roles = store.list(&ctx, Some(filter), None).await?;

        // -- Assert
        assert_eq!(roles.len(), 1);
        assert_eq!(roles[0].name, "list-role-b");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_many_to_many_ok() -> Result<()> {
        // -- Setup
        let role_id = DbId::from(Uuid::new_v4());

        let joined_perm = |name: &str| JoinedPermissionOnRole {
            id: DbId::from(Uuid::new_v4()),
            workspace_id: Uuid::new_v4(),
            name: name.to_string(),
            code: None,
            description: None,
            tags: vec![],
            meta: PermissionMeta::default(),
            created_by: DbId::default(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_by: None,
            updated_at: None,
        };

        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<(i64,)>( (2,) )
                .with_optional::<RoleWithPermissions>(Some(RoleWithPermissions {
                    id: role_id,
                    role: RoleRow {
                        id: role_id,
                        name: "role-with-perms".into(),
                        ..role_row()
                    },
                    permissions: vec![joined_perm("entity:read"), joined_perm("entity:write")],
                })),
        );
        let store = RoleStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let role_with_perms = store.get_many_to_many(&ctx, &role_id).await?;

        // -- Assert
        assert_eq!(role_with_perms.id, role_id);
        assert_eq!(
            role_with_perms.permissions.len(),
            2,
            "Should have 2 permissions attached"
        );

        let has_read = role_with_perms
            .permissions
            .iter()
            .any(|p| p.name == "entity:read");
        let has_write = role_with_perms
            .permissions
            .iter()
            .any(|p| p.name == "entity:write");
        assert!(has_read, "Should contain the 'entity:read' permission");
        assert!(has_write, "Should contain the 'entity:write' permission");

        Ok(())
    }

    #[tokio::test]
    async fn test_filter_by_contains_tags() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<RoleRow>(vec![RoleRow {
                    name: "tags-role-a".into(),
                    tags: vec!["billing".into(), "admin".into()],
                    ..role_row()
                }])
                .with_all::<RoleRow>(vec![RoleRow {
                    name: "tags-role-b".into(),
                    tags: vec!["technical".into(), "editor".into()],
                    ..role_row()
                }]),
        );
        let store = RoleStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute & Assert
        let billing_roles = store
            .filter_by_tags_contain(&ctx, vec!["billing".into()], None)
            .await?;
        assert_eq!(
            billing_roles.len(),
            1,
            "Should find 1 role with 'billing' tag"
        );
        assert_eq!(billing_roles[0].name, "tags-role-a");

        let editor_roles = store
            .filter_by_tags_contain(&ctx, vec!["editor".into()], None)
            .await?;
        assert_eq!(
            editor_roles.len(),
            1,
            "Should find 1 role with 'editor' tag"
        );
        assert_eq!(editor_roles[0].name, "tags-role-b");

        Ok(())
    }

    #[tokio::test]
    async fn test_create_multiple_roles_unique_defaults() -> Result<()> {
        // -- Setup
        let workspace_id = Uuid::new_v4();
        let role_id = |name: &str| {
            let id = DbId::from(Uuid::new_v4());
            RoleRow {
                id,
                workspace_id,
                name: name.into(),
                ..role_row()
            }
        };

        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<RoleRow>(role_id("role-a"))
                .with_one::<RoleRow>(role_id("role-b"))
                .with_one::<RoleRow>(role_id("role-c"))
                .with_optional::<RoleRow>(Some(role_id("role-a")))
                .with_optional::<RoleRow>(Some(role_id("role-b")))
                .with_optional::<RoleRow>(Some(role_id("role-c"))),
        );
        let store = RoleStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute: create 3 roles using Default::default() with same workspace
        let role1 = store
            .create(
                &ctx,
                RoleForCreate {
                    workspace_id,
                    ..Default::default()
                },
            )
            .await?;
        let role2 = store
            .create(
                &ctx,
                RoleForCreate {
                    workspace_id,
                    ..Default::default()
                },
            )
            .await?;
        let role3 = store
            .create(
                &ctx,
                RoleForCreate {
                    workspace_id,
                    ..Default::default()
                },
            )
            .await?;

        // -- Assert: all 3 roles persisted with unique names and valid IDs
        assert_ne!(role1.id, role2.id);
        assert_ne!(role2.id, role3.id);
        assert_ne!(role1.id, role3.id);
        assert_ne!(
            role1.name, role2.name,
            "Default role names should be unique"
        );
        assert_ne!(
            role2.name, role3.name,
            "Default role names should be unique"
        );
        assert_ne!(
            role1.name, role3.name,
            "Default role names should be unique"
        );
        assert_eq!(role1.workspace_id, workspace_id);
        assert_eq!(role2.workspace_id, workspace_id);
        assert_eq!(role3.workspace_id, workspace_id);

        // Verify all 3 are retrievable
        let fetched1 = store.get(&ctx, &role1.id).await?;
        let fetched2 = store.get(&ctx, &role2.id).await?;
        let fetched3 = store.get(&ctx, &role3.id).await?;
        assert_eq!(fetched1.name, role1.name);
        assert_eq!(fetched2.name, role2.name);
        assert_eq!(fetched3.name, role3.name);

        Ok(())
    }
}
// -----------------------------------------------------------------------------
// endregion: --- Tests
