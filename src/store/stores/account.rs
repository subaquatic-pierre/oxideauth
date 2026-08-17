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
