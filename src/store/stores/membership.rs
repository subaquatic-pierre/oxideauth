use std::sync::Arc;

use modql::filter::ListOptions;

use crate::store::{
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::{
        id::DbId,
        membership::{
            MembershipFilter, MembershipForCreate, MembershipForUpdate, MembershipIden,
            MembershipRow, MembershipWithRoles,
        },
    },
    error::StoreResult,
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

/// The struct for our Membership store, holding the database connection wrapper.
pub struct MembershipStore<D: DbExecutor> {
    dbx: Arc<D>,
}

impl<D: DbExecutor> MembershipStore<D> {
    /// Creates a new `MembershipStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        Self { dbx }
    }

    /// Lists memberships whose set of linked roles **contains all** of the given
    /// role IDs (via the `membership_role` join table).
    pub async fn list_containing_roles(
        &self,
        ctx: &StoreCtx,
        role_ids: Vec<DbId>,
        tags: Option<Vec<String>>,
        filter: Option<MembershipFilter>,
        opts: Option<ListOptions>,
    ) -> StoreResult<Vec<MembershipRow>> {
        let meta = ListContainingManyQueryMeta {
            table: MembershipIden::Table,
            pk: MembershipIden::Id,
            join_table: MembershipIden::MembershipRole,
            join_fk: MembershipIden::MembershipId,
            join_many_fk: MembershipIden::RoleId,
            has_audit: true,
        };

        list_containing_many(ctx, &self.dbx, role_ids, tags, filter, opts, &meta).await
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, MembershipStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for MembershipStore<D> {
    type Iden = MembershipIden;
    type Row = MembershipRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for MembershipStore<D> {
    type FilterStoreParams = MembershipFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: MembershipIden::Table,
            pk: MembershipIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> MutateStore for MembershipStore<D> {
    type CreateStoreParams = MembershipForCreate;
    type UpdateStoreParams = MembershipForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: MembershipIden::Table,
            pk: MembershipIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ManyToManyStore for MembershipStore<D> {
    type ManyToManyRow = MembershipWithRoles;

    type FilterStoreParams = MembershipFilter;

    fn many_to_many_meta(&self) -> ManyToManyQueryMeta<Self::Iden> {
        ManyToManyQueryMeta {
            single_table: MembershipIden::Table,
            many_table: MembershipIden::Role,
            join_table: MembershipIden::MembershipRole,
            single_pk: MembershipIden::Id,
            many_pk: MembershipIden::RolePk,
            many_fk: MembershipIden::RoleId,
            join_fk: MembershipIden::MembershipId,
            agg_alias: MembershipIden::Roles,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for MembershipStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: MembershipIden::Table,
            col: MembershipIden::Tags,
            has_audit: true,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: MembershipIden::Table,
            col: MembershipIden::Meta,
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
            entities::{
                account::AccountForCreate,
                id::DbId,
                membership::{MembershipForCreate, MembershipMeta},
                role::RoleForCreate,
                workspace::WorkspaceForCreate,
            },
            error::StoreError,
            traits::{
                contains::FilterByContains,
                crud::*,
                join::{GetManyToMany, LinkManyToMany},
            },
        },
    };
    use anyhow::Result;
    use serde_json::json;
    use serial_test::serial;

    /// Helper function to seed the necessary Account and Space for a Membership.
    async fn seed_prerequisites(
        ctx: &StoreCtx,
        app: &crate::app::AppState<PgDbx, RedisChx>,
    ) -> Result<(uuid::Uuid, uuid::Uuid)> {
        let account = app
            .sm
            .account
            .create(ctx, AccountForCreate::default())
            .await?;
        let workspace = app
            .sm
            .workspace
            .create(ctx, WorkspaceForCreate::default())
            .await?;
        Ok((account.id.into(), workspace.id.into()))
    }

    #[tokio::test]
    #[serial]
    async fn test_create_get_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = MembershipStore::new(dbx);
        let ctx = StoreCtx::bootstrap();
        let (account_id, workspace_id) = seed_prerequisites(&ctx, &app).await?;

        let data = MembershipForCreate {
            account_id,
            workspace_id,
            ..Default::default()
        };

        // -- Execute
        let created_membership = store.create(&ctx, data).await?;
        let fetched_membership = store.get(&ctx, &created_membership.id).await?;

        // -- Assert
        assert_eq!(created_membership.account_id, account_id);
        assert_eq!(created_membership.workspace_id, workspace_id);
        assert_eq!(fetched_membership.id, created_membership.id);
        assert_eq!(fetched_membership.account_id, created_membership.account_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_update_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = MembershipStore::new(dbx);
        let ctx = StoreCtx::bootstrap();
        let (account_id, workspace_id) = seed_prerequisites(&ctx, &app).await?;

        let created_membership = store
            .create(
                &ctx,
                MembershipForCreate {
                    account_id,
                    workspace_id,
                    ..Default::default()
                },
            )
            .await?;

        let update_data = MembershipForUpdate {
            meta: Some(MembershipMeta {
                schema_version: "1".to_string(),
            }),
            ..Default::default()
        };

        // -- Execute
        let updated_membership = store
            .update(&ctx, &created_membership.id, update_data)
            .await?;
        let fetched_membership = store.get(&ctx, &created_membership.id).await?;

        // -- Assert
        assert_eq!(updated_membership.meta.schema_version, "1");
        assert_eq!(fetched_membership.meta.schema_version, "1");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = MembershipStore::new(dbx);
        let ctx = StoreCtx::bootstrap();
        let (account_id, workspace_id) = seed_prerequisites(&ctx, &app).await?;

        let created_membership = store
            .create(
                &ctx,
                MembershipForCreate {
                    account_id,
                    workspace_id,
                    ..Default::default()
                },
            )
            .await?;

        // -- Execute
        let deleted_membership = store.delete(&ctx, &created_membership.id).await?;
        let get_result = store.get(&ctx, &created_membership.id).await;

        // -- Assert
        assert_eq!(deleted_membership.id, created_membership.id);
        assert!(
            matches!(get_result, Err(StoreError::EntityNotFound { .. })),
            "Getting the membership after deletion should fail"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_list_with_filter_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = MembershipStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let (account_id_1, space_id_1) = seed_prerequisites(&ctx, &app).await?;
        let (account_id_2, space_id_2) = seed_prerequisites(&ctx, &app).await?;

        let memberships_to_create = vec![
            MembershipForCreate {
                account_id: account_id_1,
                workspace_id: space_id_1,
                ..Default::default()
            },
            MembershipForCreate {
                account_id: account_id_2,
                workspace_id: space_id_2,
                ..Default::default()
            },
        ];
        store.create_many(&ctx, memberships_to_create).await?;

        // -- Execute
        let filter: MembershipFilter = json!({ "account_id": account_id_2 }).try_into()?;
        let memberships = store.list(&ctx, Some(filter), None).await?;

        // -- Assert
        assert_eq!(memberships.len(), 1);
        assert_eq!(memberships[0].account_id, account_id_2);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_get_many_to_many_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = MembershipStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let (account_id, workspace_id) = seed_prerequisites(&ctx, &app).await?;
        let membership = store
            .create(
                &ctx,
                MembershipForCreate {
                    account_id,
                    workspace_id,
                    ..Default::default()
                },
            )
            .await?;

        // Create and link roles
        let role_admin = app
            .sm
            .role
            .create(
                &ctx,
                RoleForCreate {
                    name: "admin".into(),
                    workspace_id,
                    ..Default::default()
                },
            )
            .await?;
        let role_editor = app
            .sm
            .role
            .create(
                &ctx,
                RoleForCreate {
                    name: "editor".into(),
                    workspace_id,
                    ..Default::default()
                },
            )
            .await?;

        app.sm
            .membership
            .attach_link(&ctx, &membership.id, &role_admin.id)
            .await?;

        app.sm
            .membership
            .attach_link(&ctx, &membership.id, &role_editor.id)
            .await?;
        // -- Execute
        let membership_with_roles = store.get_many_to_many(&ctx, &membership.id).await?;

        // -- Assert
        assert_eq!(membership_with_roles.id, membership.id);
        assert_eq!(
            membership_with_roles.roles.len(),
            2,
            "Should have 2 roles attached"
        );

        let has_admin = membership_with_roles
            .roles
            .iter()
            .any(|r| r.name == "admin");
        let has_editor = membership_with_roles
            .roles
            .iter()
            .any(|r| r.name == "editor");
        assert!(has_admin, "Should contain the 'admin' role");
        assert!(has_editor, "Should contain the 'editor' role");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_filter_by_contains_tags() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = MembershipStore::new(dbx);
        let ctx = StoreCtx::bootstrap();
        let (account_id, workspace_id) = seed_prerequisites(&ctx, &app).await?;

        // -- Create test data with different tags
        store
            .create(
                &ctx,
                MembershipForCreate {
                    account_id,
                    workspace_id,
                    tags: vec!["system".into(), "critical".into()],
                    ..Default::default()
                },
            )
            .await?;

        let (account_id, workspace_id) = seed_prerequisites(&ctx, &app).await?;
        store
            .create(
                &ctx,
                MembershipForCreate {
                    account_id,
                    workspace_id,
                    tags: vec!["user".into(), "general".into()],
                    ..Default::default()
                },
            )
            .await?;

        // -- Execute & Assert
        // NOTE: the programmatic seed creates 6 memberships all tagged "system",
        // so filtering by "system" would match seed data. Use unique tags instead.
        let critical_memberships = store
            .filter_by_tags_contain(&ctx, vec!["critical".into()], None)
            .await?;
        assert_eq!(
            critical_memberships.len(),
            1,
            "Should find 1 membership with 'critical' tag"
        );
        assert!(
            critical_memberships[0]
                .tags
                .contains(&"critical".to_string())
        );

        let general_memberships = store
            .filter_by_tags_contain(&ctx, vec!["general".into()], None)
            .await?;
        assert_eq!(
            general_memberships.len(),
            1,
            "Should find 1 membership with 'general' tag"
        );
        assert!(general_memberships[0].tags.contains(&"general".to_string()));

        Ok(())
    }
}
// -----------------------------------------------------------------------------
// endregion: --- Tests
