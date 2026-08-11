use std::sync::Arc;

use serde_json::json;

use crate::store::{
    crud::List,
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::account::{
        AccountFilter, AccountForCreate, AccountForUpdate, AccountIden, AccountRow,
        AccountWithCredentials,
    },
    error::{StoreError, StoreResult},
    queries::meta::{ContainsFilterQueryMeta, MutateQueryMeta, OneToManyQueryMeta, ReadQueryMeta},
    stores::workspace::SYSTEM_CONST,
    traits::{
        dbx::DbExecutor,
        meta::{ContainsFilterStore, MutateStore, OneToManyStore, ReadStore, Store},
    },
};

/// The struct for our Account store, holding the database connection wrapper.
pub struct AccountStore<D: DbExecutor> {
    dbx: Arc<D>,
}

impl<D: DbExecutor> AccountStore<D> {
    /// Creates a new `AccountStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        Self { dbx }
    }

    pub async fn get_system_acc(&self, ctx: &StoreCtx) -> StoreResult<AccountRow> {
        let acc = self
            .get_by_email(ctx, SYSTEM_CONST.system_acc_email)
            .await?
            .ok_or_else(|| StoreError::EntityNotFound {
                entity: "account".to_string(),
                id: SYSTEM_CONST.system_acc_email.to_string(),
            })?;
        Ok(acc)
    }

    pub async fn get_by_email(
        &self,
        ctx: &StoreCtx,
        email: &str,
    ) -> StoreResult<Option<AccountRow>> {
        let filter: AccountFilter = json!({
            "email": email.to_string()
        })
        .try_into()?;

        Ok(self.list(ctx, Some(filter), None).await?.into_iter().next())
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, AccountStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for AccountStore<D> {
    type Iden = AccountIden;
    type Row = AccountRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for AccountStore<D> {
    type FilterStoreParams = AccountFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: AccountIden::Table,
            pk: AccountIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> MutateStore for AccountStore<D> {
    type CreateStoreParams = AccountForCreate;
    type UpdateStoreParams = AccountForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: AccountIden::Table,
            pk: AccountIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> OneToManyStore for AccountStore<D> {
    type OneToManyRow = AccountWithCredentials;

    type FilterStoreParams = AccountFilter;

    fn one_to_many_meta(&self) -> OneToManyQueryMeta<Self::Iden> {
        OneToManyQueryMeta {
            single_table: AccountIden::Table,
            many_table: AccountIden::Credential,
            single_pk: AccountIden::Id,
            many_pk: AccountIden::Id,
            many_fk: AccountIden::AccountId,
            agg_alias: AccountIden::Credentials,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for AccountStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: AccountIden::Table,
            col: AccountIden::Tags,
            has_audit: true,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: AccountIden::Table,
            col: AccountIden::Meta,
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
        dev::init::init_test,
        store::{
            ctx::StoreCtx,
            entities::{
                credential::{CredentialForCreate, CredentialKind, CredentialProvider},
                workspace::WorkspaceForCreate,
            },
            error::StoreError,
            traits::{contains::FilterByContains, crud::*, join::GetOneToMany},
        },
    };
    use anyhow::Result;
    use modql::filter::{ListOptions, OpValsString};
    use serde_json::json;
    use serial_test::serial;
    use uuid::Uuid;

    #[tokio::test]
    #[serial]
    async fn test_create_get_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx();
        let store = AccountStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let data = AccountForCreate {
            email: "create-get@example.com".to_string(),
            name: "Test User".to_string(),
            ..Default::default()
        };

        // -- Execute
        let created_account = store.create(&ctx, data).await?;
        let fetched_account = store.get(&ctx, &created_account.id).await?;

        // -- Assert
        assert_eq!(created_account.email, "create-get@example.com");
        assert_eq!(fetched_account.id, created_account.id);
        assert_eq!(fetched_account.name, created_account.name);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_update_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx();
        let store = AccountStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let created_account = store.create(&ctx, AccountForCreate::default()).await?;

        let update_data = AccountForUpdate {
            name: Some("User After Update".to_string()),
            ..Default::default()
        };

        // -- Execute
        let updated_account = store.update(&ctx, &created_account.id, update_data).await?;
        let fetched_account = store.get(&ctx, &created_account.id).await?;

        // -- Assert
        assert_eq!(updated_account.name, "User After Update");
        assert_eq!(fetched_account.name, "User After Update");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx();
        let store = AccountStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let created_account = store.create(&ctx, AccountForCreate::default()).await?;

        // -- Execute
        let deleted_account = store.delete(&ctx, &created_account.id).await?;
        let get_result = store.get(&ctx, &created_account.id).await;

        // -- Assert
        assert_eq!(deleted_account.id, created_account.id);
        assert!(
            matches!(get_result, Err(StoreError::EntityNotFound { .. })),
            "Getting the account after deletion should fail"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_list_with_filter_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx();
        let store = AccountStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let accounts_to_create = vec![
            AccountForCreate {
                email: "list-a@example.com".to_string(),
                ..Default::default()
            },
            AccountForCreate {
                email: "list-b@example.com".to_string(),
                ..Default::default()
            },
        ];
        store.create_many(&ctx, accounts_to_create).await?;

        // -- Execute
        let filter: AccountFilter = json!({"email":{"$contains":"list-b"}}).try_into()?;
        let accounts = store.list(&ctx, Some(filter), None).await?;

        // -- Assert
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].email, "list-b@example.com");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_get_one_to_many_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx();
        let store = AccountStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let account = store
            .create(
                &ctx,
                AccountForCreate {
                    email: "one-to-many@example.com".into(),
                    ..Default::default()
                },
            )
            .await?;

        // Create a valid workspace to scope the credentials under (StoreCtx::bootstrap()
        // uses a nil workspace_id, which no longer references a seeded workspace).
        let workspace = app
            .sm
            .workspace
            .create(&ctx, WorkspaceForCreate::default())
            .await?;

        // Manually insert related credentials
        let mut n_cred = CredentialForCreate::default();
        n_cred.account_id = account.id.into();
        n_cred.workspace_id = workspace.id.into();
        // Google is an OAuth provider; keep it out of the unique active-password
        // index (one active password credential per workspace+account).
        n_cred.kind = CredentialKind::OAuth;
        n_cred.provider = CredentialProvider::Google;
        app.sm.credential.create(&ctx, n_cred).await?;

        let mut n_cred = CredentialForCreate::default();
        n_cred.account_id = account.id.into();
        n_cred.workspace_id = workspace.id.into();
        n_cred.provider = CredentialProvider::Local;
        app.sm.credential.create(&ctx, n_cred).await?;

        // -- Execute
        let account_with_creds = store.get_one_to_many(&ctx, &account.id).await?;

        // -- Assert
        assert_eq!(account_with_creds.id, account.id);
        assert_eq!(
            account_with_creds.credentials.len(),
            2,
            "Should have 2 credentials attached"
        );

        let has_google = account_with_creds
            .credentials
            .iter()
            .any(|c| c.provider == CredentialProvider::Google);
        let has_local = account_with_creds
            .credentials
            .iter()
            .any(|c| c.provider == CredentialProvider::Local);
        assert!(has_google, "Should contain a google credential");
        assert!(has_local, "Should contain a local credential");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_filter_by_contains_tags() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx();
        let store = AccountStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Create test data with different tags
        // NOTE: tags are prefixed with "test-filter-" to avoid colliding with the
        // seed data (the seeded system/owner accounts carry a "system" tag).
        store
            .create(
                &ctx,
                AccountForCreate {
                    email: "tags-a@example.com".into(),
                    tags: vec!["test-filter-system".into(), "test-filter-critical".into()],
                    ..Default::default()
                },
            )
            .await?;
        store
            .create(
                &ctx,
                AccountForCreate {
                    email: "tags-b@example.com".into(),
                    tags: vec!["test-filter-user".into(), "test-filter-general".into()],
                    ..Default::default()
                },
            )
            .await?;

        // -- Execute & Assert
        let system_accounts = store
            .filter_by_tags_contain(&ctx, vec!["test-filter-system".into()], None)
            .await?;
        assert_eq!(
            system_accounts.len(),
            1,
            "Should find 1 account with 'test-filter-system' tag"
        );
        assert_eq!(system_accounts[0].email, "tags-a@example.com");

        let general_accounts = store
            .filter_by_tags_contain(&ctx, vec!["test-filter-general".into()], None)
            .await?;
        assert_eq!(
            general_accounts.len(),
            1,
            "Should find 1 account with 'test-filter-general' tag"
        );
        assert_eq!(general_accounts[0].email, "tags-b@example.com");

        Ok(())
    }
}
// -----------------------------------------------------------------------------
// endregion: --- Tests
