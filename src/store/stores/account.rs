use std::sync::Arc;

use modql::filter::ListOptions;
use serde_json::json;

use crate::store::{
    crud::List,
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::{
        account::{
            AccountFilter, AccountForCreate, AccountForUpdate, AccountIden, AccountRow,
            AccountWithCredentials,
        },
        membership::MembershipIden,
    },
    error::{StoreError, StoreResult},
    queries::{
        list::list_in_namespace_by_join_table,
        meta::{
            ContainsFilterQueryMeta, ListInNamespaceQueryMeta, MutateQueryMeta, OneToManyQueryMeta,
            ReadQueryMeta,
        },
    },
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

    /// Lists accounts that belong to the current namespace (workspace) by
    /// joining on their memberships.
    ///
    /// Accounts are globally scoped (the `account` table has no `workspace_id`
    /// column), so the namespace boundary is enforced through the `membership`
    /// join table's `workspace_id`.
    pub async fn list_in_namespace_by_join_table(
        &self,
        ctx: &StoreCtx,
        tags: Option<Vec<String>>,
        filter: Option<AccountFilter>,
        opts: Option<ListOptions>,
    ) -> StoreResult<Vec<AccountRow>> {
        let meta = ListInNamespaceQueryMeta {
            table: AccountIden::Table,
            pk: AccountIden::Id,
            join_table: AccountIden::Membership,
            join_fk: AccountIden::AccountId,
            has_audit: true,
        };

        list_in_namespace_by_join_table(ctx, &self.dbx, tags, filter, opts, &meta).await
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
    use crate::store::{
        dbx::MockDbx,
        entities::{
            account::JoinedCredentialOnAccount,
            credential::{CredentialKind, CredentialProvider, CredentialStatus},
            id::DbId,
        },
        error::StoreError,
        traits::{contains::FilterByContains, crud::*, join::GetOneToMany},
    };
    use anyhow::Result;
    use serde_json::json;
    use time::OffsetDateTime;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_create_get_ok() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<AccountRow>(AccountRow {
                    email: "create-get@example.com".into(),
                    name: "Test User".into(),
                    ..Default::default()
                })
                .with_optional::<AccountRow>(Some(AccountRow {
                    email: "create-get@example.com".into(),
                    name: "Test User".into(),
                    ..Default::default()
                })),
        );
        let store = AccountStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let created_account = store.create(&ctx, AccountForCreate::default()).await?;
        let fetched_account = store.get(&ctx, &created_account.id).await?;

        // -- Assert
        assert_eq!(created_account.email, "create-get@example.com");
        assert_eq!(fetched_account.id, created_account.id);
        assert_eq!(fetched_account.name, "Test User");

        Ok(())
    }

    #[tokio::test]
    async fn test_update_ok() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<AccountRow>(AccountRow {
                    name: "Test User".into(),
                    ..Default::default()
                })
                .with_optional::<AccountRow>(Some(AccountRow {
                    name: "User After Update".into(),
                    ..Default::default()
                }))
                .with_optional::<AccountRow>(Some(AccountRow {
                    name: "User After Update".into(),
                    ..Default::default()
                })),
        );
        let store = AccountStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let created_account = store.create(&ctx, AccountForCreate::default()).await?;
        let updated_account = store
            .update(
                &ctx,
                &created_account.id,
                AccountForUpdate {
                    name: Some("User After Update".to_string()),
                    ..Default::default()
                },
            )
            .await?;
        let fetched_account = store.get(&ctx, &created_account.id).await?;

        // -- Assert
        assert_eq!(updated_account.name, "User After Update");
        assert_eq!(fetched_account.name, "User After Update");

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_ok() -> Result<()> {
        // -- Setup
        let account_id = DbId::from(Uuid::new_v4());
        let dbx = Arc::new(
            MockDbx::new()
                .with_optional::<AccountRow>(Some(AccountRow {
                    id: account_id,
                    ..Default::default()
                }))
                .with_optional::<AccountRow>(None),
        );
        let store = AccountStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let deleted_account = store.delete(&ctx, &account_id).await?;
        let get_result = store.get(&ctx, &account_id).await;

        // -- Assert
        assert_eq!(deleted_account.id, account_id);
        assert!(
            matches!(get_result, Err(StoreError::EntityNotFound { .. })),
            "Getting the account after deletion should fail"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_list_with_filter_ok() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<AccountRow>(vec![AccountRow::default(), AccountRow::default()])
                .with_all::<AccountRow>(vec![AccountRow {
                    email: "list-b@example.com".into(),
                    ..Default::default()
                }]),
        );
        let store = AccountStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        store
            .create_many(
                &ctx,
                vec![AccountForCreate::default(), AccountForCreate::default()],
            )
            .await?;

        let filter: AccountFilter = json!({"email":{"$contains":"list-b"}}).try_into()?;
        let accounts = store.list(&ctx, Some(filter), None).await?;

        // -- Assert
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].email, "list-b@example.com");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_one_to_many_ok() -> Result<()> {
        // -- Setup
        let account_id = DbId::from(Uuid::new_v4());

        let cred = |provider| JoinedCredentialOnAccount {
            id: DbId::from(Uuid::new_v4()),
            account_id,
            workspace_id: DbId::from(Uuid::new_v4()),
            kind: CredentialKind::OAuth,
            provider,
            status: CredentialStatus::Active,
            provider_id: None,
            email: None,
            secret: None,
            last_used_at: None,
            tags: vec![],
            created_by: DbId::default(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_by: None,
            updated_at: None,
        };

        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<(i64,)>( (2,) )
                .with_optional::<AccountWithCredentials>(Some(AccountWithCredentials {
                    id: account_id,
                    account: AccountRow {
                        id: account_id,
                        email: "one-to-many@example.com".into(),
                        ..Default::default()
                    },
                    credentials: vec![
                        cred(CredentialProvider::Google),
                        cred(CredentialProvider::Local),
                    ],
                })),
        );
        let store = AccountStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let account_with_creds = store.get_one_to_many(&ctx, &account_id).await?;

        // -- Assert
        assert_eq!(account_with_creds.id, account_id);
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
    async fn test_filter_by_contains_tags() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<AccountRow>(vec![AccountRow {
                    email: "tags-a@example.com".into(),
                    tags: vec!["test-filter-system".into(), "test-filter-critical".into()],
                    ..Default::default()
                }])
                .with_all::<AccountRow>(vec![AccountRow {
                    email: "tags-b@example.com".into(),
                    tags: vec!["test-filter-user".into(), "test-filter-general".into()],
                    ..Default::default()
                }]),
        );
        let store = AccountStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

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
