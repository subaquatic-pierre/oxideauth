use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::{
                Account, AccountCreateParams, AccountDeleteParams, AccountDescribeParams,
                AccountListParams, AccountUpdateParams,
            },
            list::{ListResponse, ListResponseMeta},
            membership::MembershipFilter,
            permission::PermissionRule,
            workspace::{Workspace, WorkspaceDescribeParams},
        },
        services::{
            validator::AuthValidator, permission::CANONICAL_PERMISSIONS, workspace::WorkspaceService,
        },
        traits::{
            list::RequestListParams,
            service::{
                CoreModelCreateService, CoreModelDeleteService, CoreModelDescribeService,
                CoreModelListService, CoreModelService, CoreModelUpdateService,
            },
        },
    },
    store::{
        contains::FilterByContains,
        ctx::StoreCtx,
        dbx::PgDbx,
        entities::{
            account::{
                AccountFilter, AccountForCreate, AccountForUpdate, AccountKind, AccountMeta,
            },
            id::DbId,
        },
        error::StoreError,
        manager::StoreManager,
        meta::{ContainsFilterStore, StoreId},
        stores::account::AccountStore,
        traits::{crud::*, dbx::DbExecutor},
        utils::ListOptionsValidator,
    },
};

pub struct AccountService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    ws_svc: Arc<WorkspaceService<D, C>>,
    validator: Arc<AuthValidator>,
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D, C> for AccountService<D, C> {
    type CoreModel = Account;
    type ServiceStore = AccountStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.account
    }

    fn ws_svc(&self) -> &WorkspaceService<D, C> {
        self.ws_svc.as_ref()
    }

    fn validator(&self) -> &AuthValidator {
        self.validator.as_ref()
    }

    fn is_scoped(&self) -> bool {
        false
    }
}

impl<D: DbExecutor, C: CacheExecutor> AccountService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        cm: Arc<CacheManager<C>>,
        ws_svc: Arc<WorkspaceService<D, C>>,
        validator: Arc<AuthValidator>,
    ) -> Self {
        Self {
            sm,
            cm,
            ws_svc,
            validator,
        }
    }

    async fn get_account_by_id_or_email(
        &self,
        ctx: &mut CoreCtx,
        id_or_email: &str,
    ) -> CoreResult<Account> {
        let store = self.store();
        // NOTE(workspace-scope): unscoped — the account table has no workspace_id column.
        let mut store_ctx: StoreCtx = ctx.into();
        store_ctx.set_workspace_scope(None);

        let acc = match Uuid::parse_str(&id_or_email) {
            Ok(id) => self.store().get(&store_ctx, &id.into()).await?.into(),
            Err(_) => self
                .get_by_email(
                    ctx,
                    &AccountDescribeParams {
                        id: None,
                        email: Some(id_or_email.to_string()),
                    },
                )
                .await?
                .ok_or(CoreError::NotFound(format!(
                    "Unable to get account id email: {id_or_email}"
                )))?,
        };

        Ok(acc)
    }

    /// Resolves an account from a typed id-or-email descriptor.
    ///
    /// When `params.id` is present the account is fetched by id (returning
    /// `Ok(None)` if it does not exist), otherwise `params.email` is used for
    /// an email lookup.
    pub async fn get_by_email(
        &self,
        ctx: &CoreCtx,
        params: &AccountDescribeParams,
    ) -> CoreResult<Option<Account>> {
        let store = self.store();

        // NOTE(workspace-scope): unscoped — the account table has no workspace_id column.
        let mut store_ctx: StoreCtx = ctx.into();
        store_ctx.set_workspace_scope(None);

        let acc = match params.id {
            Some(id) => store.get_opt(&store_ctx, &id.into()).await?,
            None => {
                let Some(email) = params.email.as_deref() else {
                    return Ok(None);
                };
                store.get_by_email(&store_ctx, email).await?
            }
        };

        Ok(acc.map(|el| el.into()))
    }

    async fn invalidate_all_memberships(&self, ctx: &CoreCtx, acc_id: Uuid) -> CoreResult<()> {
        // NOTE(workspace-scope): unscoped — the account table has no workspace_id column.
        let mut store_ctx: StoreCtx = ctx.into();
        store_ctx.set_workspace_scope(None);
        let filter = json!({"account_id":&acc_id.to_string()}).try_into()?;
        // list all memberships with account in workspace all workspaces
        let memberships = self
            .sm
            .membership
            .list(&store_ctx, Some(filter), None)
            .await?;

        for mem in memberships {
            self.cm.auth.invalidate(mem.id.into()).await?;
        }

        Ok(())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D, C> for AccountService<D, C> {
    type CreateParams = AccountCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.account.create;

    async fn create(&self, ctx: &mut CoreCtx, params: AccountCreateParams) -> CoreResult<Account> {
        let store = self.store();

        // NOTE(workspace-scope): unscoped - global table (no workspace_id column).
        let store_ctx = self.scope_and_validate(ctx, None, &[Self::CREATE_PERMISSION]).await?;
        if store
            .get_by_email(&store_ctx, &params.email)
            .await?
            .is_some()
        {
            return Err(CoreError::AlreadyExists(format!(
                "Account with email: {} already exists",
                params.email
            )));
        }

        let new_account = store.create(&store_ctx, params.into()).await?;

        Ok(new_account.into())
    }
}
impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D, C> for AccountService<D, C> {
    type DescribeParams = AccountDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.account.describe;

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: AccountDescribeParams,
    ) -> CoreResult<Account> {
        let store = self.store();

        // NOTE(workspace-scope): unscoped - global table (no workspace_id column).
        let store_ctx = self.scope_and_validate(ctx, None, &[Self::DESCRIBE_PERMISSION]).await?;

        let identifier =
            params
                .id
                .map(|id| id.to_string())
                .or(params.email)
                .ok_or(CoreError::InvalidParams(
                    "Email or ID required to describe account".to_string(),
                ))?;

        self.get_account_by_id_or_email(ctx, &identifier).await
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D, C> for AccountService<D, C> {
    type ListParams = AccountListParams;
    const LIST_PERMISSION: &'static str = CANONICAL_PERMISSIONS.account.list;

    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: AccountListParams,
    ) -> CoreResult<ListResponse<Account>> {
        let store = self.store();

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(ctx.scoped_ws_id()), &[Self::LIST_PERMISSION])
            .await?;

        let options = params.list_options();

        let tags_filter = params.validate_filter_tags()?;

        let tags = tags_filter.tags();
        let filter = tags_filter.filter();

        let data = store
            .list_in_namespace_by_join_table(
                &store_ctx,
                tags.clone(),
                filter.clone(),
                Some(options.clone()),
            )
            .await?;
        let total = store
            .count_with_tags_and_filter(&store_ctx, tags, filter)
            .await?;

        let accounts: Vec<Account> = data.into_iter().map(|el| el.into()).collect();
        Ok(ListResponse::new(accounts, total, options))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D, C> for AccountService<D, C> {
    type UpdateParams = AccountUpdateParams;
    const UPDATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.account.update;

    async fn update(&self, ctx: &mut CoreCtx, params: AccountUpdateParams) -> CoreResult<Account> {
        let store = self.store();
        // NOTE(workspace-scope): unscoped - global table (no workspace_id column).
        let store_ctx = self.scope_and_validate(ctx, None, &[Self::UPDATE_PERMISSION]).await?;

        // NOTE: updating email constraints need to be enforced
        // currently cannot change account email
        let identifier = params.id_or_email()?;

        let current = self.get_account_by_id_or_email(ctx, &identifier).await?;
        let id = current.id;

        let security_change = params.enabled.map_or(false, |e| e != current.enabled);

        let new_version = if security_change {
            Some(current.version + 1)
        } else {
            Some(current.version)
        };

        let mut update_data: AccountForUpdate = params.into_store_params(new_version);

        let updated_account = store.update(&store_ctx, &id.into(), update_data).await?;

        if security_change {
            self.invalidate_all_memberships(ctx, updated_account.id.into());
        } else {
            tracing::debug!("non-security update, cache not invalidated");
        }

        Ok(updated_account.into())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D, C> for AccountService<D, C> {
    type DeleteParams = AccountDeleteParams;
    const DELETE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.account.delete;

    async fn delete(&self, ctx: &mut CoreCtx, params: AccountDeleteParams) -> CoreResult<Account> {
        let store = self.store();

        // NOTE(workspace-scope): unscoped - global table (no workspace_id column).
        let store_ctx = self.scope_and_validate(ctx, None, &[Self::DELETE_PERMISSION]).await?;

        let identifier = params.id_or_email()?;
        let acc = self.get_account_by_id_or_email(ctx, &identifier).await?;

        let deleted = store.delete(&store_ctx, &acc.id.into()).await?.into();

        let account_id: Uuid = acc.id.into();
        self.invalidate_all_memberships(ctx, account_id);

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        cache::{manager::CacheManager, mock::MockChx},
        config::Config,
        core::services::registry::ServiceRegistry,
        store::{dbx::MockDbx, entities::account::AccountRow},
    };
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_account_list() -> CoreResult<()> {
        let config = Config::test_config();

        // build store manager backed by an in-memory fake database
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<AccountRow>(vec![AccountRow {
                    email: "list@example.com".into(),
                    ..Default::default()
                }])
                .with_one::<(i64,)>((1,)),
        );
        let sm = Arc::new(StoreManager::new(dbx));

        // build cache manager with an in-memory cache (no real Redis connection)
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.account.clone();

        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["account:list"])?;

        let accounts = svc
            .list(
                &mut ctx,
                AccountListParams {
                    filter: None,
                    options: None,
                },
            )
            .await?;

        assert_eq!(accounts.data.len(), 1);
        assert_eq!(accounts.metadata.total, 1);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_create_mock_account_success() -> CoreResult<()> {
        let config = Config::test_config();

        // build store manager backed by an in-memory fake database
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<AccountRow>(vec![]) // duplicate-email check -> none found
                .with_one::<AccountRow>(AccountRow::default()), // INSERT ... RETURNING
        );
        let sm = Arc::new(StoreManager::new(dbx));

        // build cache manager with an in-memory cache (no real Redis connection)
        let mock_cache = Arc::new(MockChx::default());
        let cm = Arc::new(CacheManager::new(mock_cache));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.account.clone();
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["account:create"])?;

        let params = AccountCreateParams::default();

        let new_acc = svc.create(&mut ctx, params).await?;

        let expected = Account::default();

        assert_eq!(
            new_acc.id, expected.id,
            "incorrect account id returned from AccountService.create()"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_create_mock_account_error() -> CoreResult<()> {
        let config = Config::test_config();

        // A matching account already exists -> the duplicate-email check finds it
        let existing = AccountRow {
            email: "user@user.com".to_string(),
            ..Default::default()
        };

        let dbx = Arc::new(MockDbx::new().with_all::<AccountRow>(vec![existing]));
        let sm = Arc::new(StoreManager::new(dbx));

        // build cache manager with an in-memory cache (no real Redis connection)
        let mock_cache = Arc::new(MockChx::default());
        let cm = Arc::new(CacheManager::new(mock_cache));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.account.clone();

        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["account:create"])?;

        let params = AccountCreateParams::default();
        let new_acc = svc.create(&mut ctx, params).await;

        assert!(
            matches!(new_acc, Err(CoreError::AlreadyExists(..))),
            "should be CoreError::AlreadyExists"
        );

        Ok(())
    }
}
