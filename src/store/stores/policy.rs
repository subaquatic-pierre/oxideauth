use std::sync::Arc;

use sea_query::Iden;
use serde_json::json;

use crate::store::{
    crud::List,
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::{
        id::DbId,
        policy::{
            PolicyFilter, PolicyForCreate, PolicyForUpdate, PolicyIden, PolicyRow,
        },
    },
    error::StoreResult,
    queries::meta::{
        ContainsFilterQueryMeta, MutateQueryMeta, ReadQueryMeta,
    },
    traits::{
        dbx::DbExecutor,
        meta::{ContainsFilterStore, MutateStore, ReadStore, Store},
    },
};

/// The struct for our Policy store, holding the database connection wrapper.
///
/// NOTE: `runtime_key` uniqueness per workspace (FR-003) is **not** enforced at
/// the store layer (the key is derived, not stored). It is validated by the
/// service layer (`PolicyService`, T012).
pub struct PolicyStore<D: DbExecutor> {
    dbx: Arc<D>,
}

impl<D: DbExecutor> PolicyStore<D> {
    /// Creates a new `PolicyStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        Self { dbx }
    }

    /// Looks up a policy by its workspace-scoped `name` (unique per workspace
    /// when present).
    pub async fn get_by_name_opt(
        &self,
        ctx: &StoreCtx,
        name: &str,
        workspace_id: DbId,
    ) -> StoreResult<Option<PolicyRow>> {
        let filter: PolicyFilter = json!({
            "name": name.to_string(),
            "workspace_id": workspace_id.to_string()
        })
        .try_into()?;

        Ok(self.list(ctx, Some(filter), None).await?.into_iter().next())
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, PolicyStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for PolicyStore<D> {
    type Iden = PolicyIden;
    type Row = PolicyRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for PolicyStore<D> {
    type FilterStoreParams = PolicyFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: PolicyIden::Table,
            pk: PolicyIden::Id,
            has_audit: false,
        }
    }
}

impl<D: DbExecutor> MutateStore for PolicyStore<D> {
    type CreateStoreParams = PolicyForCreate;
    type UpdateStoreParams = PolicyForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: PolicyIden::Table,
            pk: PolicyIden::Id,
            has_audit: false,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for PolicyStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: PolicyIden::Table,
            col: PolicyIden::Tags,
            has_audit: false,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: PolicyIden::Table,
            col: PolicyIden::Meta,
            has_audit: false,
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
        entities::policy::{PolicyEffect, PolicyMeta},
        error::StoreError,
        traits::crud::*,
    };
    use anyhow::Result;
    use serde_json::json;
    use uuid::Uuid;

    /// Helper to build a `PolicyRow` with default-ish values for the mock.
    fn policy_row() -> PolicyRow {
        PolicyRow {
            id: DbId::from(Uuid::new_v4()),
            workspace_id: Uuid::new_v4(),
            name: Some("self-update".to_string()),
            effect: PolicyEffect::Allow,
            principal_id: None,
            actions: vec!["membership:update".to_string()],
            resource: "self".to_string(),
            constraint_expr: Some("membership.account.id === user.id".to_string()),
            description: None,
            tags: vec![],
            meta: PolicyMeta::default(),
            created_at: time::OffsetDateTime::UNIX_EPOCH,
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn test_create_get_ok() -> Result<()> {
        // -- Setup
        let workspace_id = Uuid::new_v4();
        let policy_id = DbId::from(Uuid::new_v4());
        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<PolicyRow>(PolicyRow {
                    id: policy_id,
                    workspace_id,
                    ..policy_row()
                })
                .with_optional::<PolicyRow>(Some(PolicyRow {
                    id: policy_id,
                    workspace_id,
                    ..policy_row()
                })),
        );
        let store = PolicyStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let data = PolicyForCreate {
            workspace_id,
            ..Default::default()
        };

        // -- Execute
        let created_policy = store.create(&ctx, data).await?;
        let fetched_policy = store.get(&ctx, &created_policy.id).await?;

        // -- Assert
        assert_eq!(created_policy.workspace_id, workspace_id);
        assert_eq!(fetched_policy.id, created_policy.id);
        assert_eq!(fetched_policy.workspace_id, workspace_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_update_ok() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<PolicyRow>(policy_row())
                .with_optional::<PolicyRow>(Some(PolicyRow {
                    name: Some("updated-policy".into()),
                    ..policy_row()
                }))
                .with_optional::<PolicyRow>(Some(PolicyRow {
                    name: Some("updated-policy".into()),
                    ..policy_row()
                })),
        );
        let store = PolicyStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let created_policy = store.create(&ctx, PolicyForCreate::default()).await?;
        let updated_policy = store
            .update(
                &ctx,
                &created_policy.id,
                PolicyForUpdate {
                    name: Some("updated-policy".to_string()),
                    ..Default::default()
                },
            )
            .await?;
        let fetched_policy = store.get(&ctx, &created_policy.id).await?;

        // -- Assert
        assert_eq!(updated_policy.name.as_deref(), Some("updated-policy"));
        assert_eq!(fetched_policy.name.as_deref(), Some("updated-policy"));

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_ok() -> Result<()> {
        // -- Setup
        let policy_id = DbId::from(Uuid::new_v4());
        let dbx = Arc::new(
            MockDbx::new()
                .with_optional::<PolicyRow>(Some(PolicyRow {
                    id: policy_id,
                    ..policy_row()
                }))
                .with_optional::<PolicyRow>(None),
        );
        let store = PolicyStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let deleted_policy = store.delete(&ctx, &policy_id).await?;
        let get_result = store.get(&ctx, &policy_id).await;

        // -- Assert
        assert_eq!(deleted_policy.id, policy_id);
        assert!(
            matches!(get_result, Err(StoreError::EntityNotFound { .. })),
            "Getting the policy after deletion should fail"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_list_with_filter_ok() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<PolicyRow>(vec![policy_row(), policy_row()])
                .with_all::<PolicyRow>(vec![PolicyRow {
                    name: Some("policy:list:b".into()),
                    ..policy_row()
                }]),
        );
        let store = PolicyStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        store
            .create_many(
                &ctx,
                vec![PolicyForCreate::default(), PolicyForCreate::default()],
            )
            .await?;

        let filter: PolicyFilter = json!({ "name": "policy:list:b" }).try_into()?;
        let policies = store.list(&ctx, Some(filter), None).await?;

        // -- Assert
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].name.as_deref(), Some("policy:list:b"));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_by_name_opt_ok() -> Result<()> {
        // -- Setup
        let workspace_id = Uuid::new_v4();
        let dbx = Arc::new(
            MockDbx::new().with_all::<PolicyRow>(vec![PolicyRow {
                name: Some("self-update".into()),
                workspace_id,
                ..policy_row()
            }]),
        );
        let store = PolicyStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let found = store
            .get_by_name_opt(&ctx, "self-update", workspace_id.into())
            .await?;

        // -- Assert
        assert!(found.is_some());
        assert_eq!(found.unwrap().name.as_deref(), Some("self-update"));

        Ok(())
    }

    #[tokio::test]
    async fn test_filter_by_contains_tags() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<PolicyRow>(vec![PolicyRow {
                    name: Some("tags-policy-a".into()),
                    tags: vec!["system".into(), "self".into()],
                    ..policy_row()
                }])
                .with_all::<PolicyRow>(vec![PolicyRow {
                    name: Some("tags-policy-b".into()),
                    tags: vec!["custom".into()],
                    ..policy_row()
                }]),
        );
        let store = PolicyStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute & Assert
        use crate::store::traits::contains::FilterByContains;
        let system_policies = store
            .filter_by_tags_contain(&ctx, vec!["system".into()], None)
            .await?;
        assert_eq!(
            system_policies.len(),
            1,
            "Should find 1 policy with 'system' tag"
        );
        assert_eq!(system_policies[0].name.as_deref(), Some("tags-policy-a"));

        Ok(())
    }
}
// -----------------------------------------------------------------------------
// endregion: --- Tests
