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
    use crate::store::{
        ctx::StoreCtx,
        dbx::MockDbx,
        entities::{
            audit::AuditFields,
            membership::{JoinedRoleOnMembership, MembershipMeta, MembershipScope, MembershipStatus},
            role::RoleMeta,
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

    /// Helper to build a `MembershipRow` with default-ish values for the mock.
    fn membership_row() -> MembershipRow {
        MembershipRow {
            id: DbId::from(Uuid::new_v4()),
            account_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            profile_id: None,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            version: 1,
            tags: vec![],
            meta: MembershipMeta::default(),
            audit: AuditFields::default(),
        }
    }

    #[tokio::test]
    async fn test_create_get_ok() -> Result<()> {
        // -- Setup
        let account_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let membership_id = DbId::from(Uuid::new_v4());

        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<MembershipRow>(MembershipRow {
                    id: membership_id,
                    account_id,
                    workspace_id,
                    ..membership_row()
                })
                .with_optional::<MembershipRow>(Some(MembershipRow {
                    id: membership_id,
                    account_id,
                    ..membership_row()
                })),
        );
        let store = MembershipStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

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
    async fn test_update_ok() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<MembershipRow>(membership_row())
                .with_optional::<MembershipRow>(Some(MembershipRow {
                    meta: MembershipMeta {
                        schema_version: "1".to_string(),
                    },
                    ..membership_row()
                }))
                .with_optional::<MembershipRow>(Some(MembershipRow {
                    meta: MembershipMeta {
                        schema_version: "1".to_string(),
                    },
                    ..membership_row()
                })),
        );
        let store = MembershipStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let created_membership = store
            .create(&ctx, MembershipForCreate::default())
            .await?;
        let updated_membership = store
            .update(
                &ctx,
                &created_membership.id,
                MembershipForUpdate {
                    meta: Some(MembershipMeta {
                        schema_version: "1".to_string(),
                    }),
                    ..Default::default()
                },
            )
            .await?;
        let fetched_membership = store.get(&ctx, &created_membership.id).await?;

        // -- Assert
        assert_eq!(updated_membership.meta.schema_version, "1");
        assert_eq!(fetched_membership.meta.schema_version, "1");

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_ok() -> Result<()> {
        // -- Setup
        let membership_id = DbId::from(Uuid::new_v4());
        let dbx = Arc::new(
            MockDbx::new()
                .with_optional::<MembershipRow>(Some(MembershipRow {
                    id: membership_id,
                    ..membership_row()
                }))
                .with_optional::<MembershipRow>(None),
        );
        let store = MembershipStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let deleted_membership = store.delete(&ctx, &membership_id).await?;
        let get_result = store.get(&ctx, &membership_id).await;

        // -- Assert
        assert_eq!(deleted_membership.id, membership_id);
        assert!(
            matches!(get_result, Err(StoreError::EntityNotFound { .. })),
            "Getting the membership after deletion should fail"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_list_with_filter_ok() -> Result<()> {
        // -- Setup
        let account_id_2 = Uuid::new_v4();
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<MembershipRow>(vec![membership_row(), membership_row()])
                .with_all::<MembershipRow>(vec![MembershipRow {
                    account_id: account_id_2,
                    ..membership_row()
                }]),
        );
        let store = MembershipStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        store
            .create_many(
                &ctx,
                vec![MembershipForCreate::default(), MembershipForCreate::default()],
            )
            .await?;

        let filter: MembershipFilter = json!({ "account_id": account_id_2 }).try_into()?;
        let memberships = store.list(&ctx, Some(filter), None).await?;

        // -- Assert
        assert_eq!(memberships.len(), 1);
        assert_eq!(memberships[0].account_id, account_id_2);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_many_to_many_ok() -> Result<()> {
        // -- Setup
        let membership_id = DbId::from(Uuid::new_v4());

        let joined_role = |name: &str| JoinedRoleOnMembership {
            id: DbId::from(Uuid::new_v4()),
            workspace_id: Uuid::new_v4(),
            name: name.to_string(),
            description: None,
            tags: vec![],
            meta: RoleMeta::default(),
            created_by: DbId::default(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_by: None,
            updated_at: None,
        };

        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<(i64,)>( (2,) )
                .with_optional::<MembershipWithRoles>(Some(MembershipWithRoles {
                    id: membership_id,
                    membership: MembershipRow {
                        id: membership_id,
                        ..membership_row()
                    },
                    roles: vec![joined_role("admin"), joined_role("editor")],
                })),
        );
        let store = MembershipStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let membership_with_roles = store.get_many_to_many(&ctx, &membership_id).await?;

        // -- Assert
        assert_eq!(membership_with_roles.id, membership_id);
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
    async fn test_filter_by_contains_tags() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<MembershipRow>(vec![MembershipRow {
                    tags: vec!["system".into(), "critical".into()],
                    ..membership_row()
                }])
                .with_all::<MembershipRow>(vec![MembershipRow {
                    tags: vec!["user".into(), "general".into()],
                    ..membership_row()
                }]),
        );
        let store = MembershipStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute & Assert
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
