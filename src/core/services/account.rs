use std::sync::Arc;

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
            permission::PermissionRule,
            workspace::{Workspace, WorkspaceDescribeParams},
        },
        services::{
            auth::AuthValidator, permission::CANONICAL_PERMISSIONS, workspace::WorkspaceService,
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

    fn should_remove_workspace_from_store_ctx(&self) -> bool {
        true
    }
}

// TODO: URGENT NOTE
// Account is the only table that is not workspace scoped
// it is important that all CRUD operations are validated
// against what accounts the requesting user is able to access
// based on the 'membership' <-> 'account' many to many join table

impl<D: DbExecutor, C: CacheExecutor> AccountService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        cm: Arc<CacheManager<C>>,
        ws_svc: Arc<WorkspaceService<D, C>>,
    ) -> Self {
        Self { sm, cm, ws_svc }
    }

    async fn get_account_id(
        &self,
        ctx: &CoreCtx,
        id: Option<Uuid>,
        email: Option<String>,
    ) -> CoreResult<DbId> {
        let store = self.store();

        let id: DbId = match (id, email) {
            (Some(id), _) => id.into(),
            (None, Some(email)) => match self.get_by_email(ctx, &email).await? {
                Some(acc) => acc.id.into(),
                None => {
                    return Err(CoreError::StoreError(StoreError::EntityNotFound {
                        entity: "account".to_string(),
                        id: email.to_string(),
                    }));
                }
            },
            (None, None) => {
                return Err(CoreError::InvalidParams(
                    "Account ID or email required".to_string(),
                ));
            }
        };

        Ok(id)
    }

    pub async fn get_by_email(&self, ctx: &CoreCtx, email: &str) -> CoreResult<Option<Account>> {
        let store = self.store();

        let acc = store.get_by_email(&ctx.into(), &email).await?;

        Ok(acc.map(|el| el.into()))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D, C> for AccountService<D, C> {
    type CreateParams = AccountCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.account.create;

    async fn create(&self, ctx: &mut CoreCtx, params: AccountCreateParams) -> CoreResult<Account> {
        let store = self.store();

        let auth_validator = AuthValidator::new(ctx);
        auth_validator.validate_ctx_perms(&[Self::CREATE_PERMISSION])?;
        let store_ctx = auth_validator.scope_store_workspace(None)?;

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

        let auth_validator = AuthValidator::new(ctx);
        auth_validator.validate_ctx_perms(&[Self::DESCRIBE_PERMISSION])?;
        let store_ctx = auth_validator.scope_store_workspace(None)?;

        let id = self.get_account_id(ctx, params.id, params.email).await?;

        let acc: Account = store.get(&store_ctx, &id).await?.into();

        Ok(acc)
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

        let auth_validator = AuthValidator::new(ctx);
        auth_validator.validate_ctx_perms(&[Self::LIST_PERMISSION])?;
        let store_ctx = auth_validator.scope_store_workspace(None)?;

        let options = params.list_options();

        let tags_filter = params.validate_filter_tags()?;

        let tags = tags_filter.tags();
        let filter = tags_filter.filter();

        let data = store
            .list_in_namespace(
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
        let auth_validator = AuthValidator::new(ctx);
        auth_validator.validate_ctx_perms(&[Self::UPDATE_PERMISSION])?;
        let store_ctx = auth_validator.scope_store_workspace(None)?;

        let id = self
            .get_account_id(&ctx, params.id, params.email.clone())
            .await?;

        // TODO: updating email constraints need to be enforced
        // if email is updated then need to set verified as false
        // ensure email does not already exist for a different account
        // force reverify
        // this email acts primarily as ID, if a user wants to update email
        // to login then can update credential used for that namespace instead

        let current = store.get(&store_ctx, &id).await?;

        let security_change = params.enabled.map_or(false, |e| e != current.enabled);

        let mut update_data: AccountForUpdate = params.into();
        if security_change {
            update_data.version = Some(current.version + 1);
        }

        let updated_account = store.update(&store_ctx, &id, update_data).await?;

        if security_change {
            let account_id: Uuid = id.into();
            // TODO: invalidate auth cache
            // get all memberships from all workspaces
            // clear cache for all memberships
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

        let auth_validator = AuthValidator::new(ctx);
        auth_validator.validate_ctx_perms(&[Self::DELETE_PERMISSION])?;
        let store_ctx = auth_validator.scope_store_workspace(None)?;

        let id = self.get_account_id(&ctx, params.id, params.email).await?;

        let deleted = store.delete(&store_ctx, &id).await?.into();

        let account_id: Uuid = id.into();
        // TODO: invalidate auth cache
        // get all memberships from all workspaces
        // clear cache for all memberships

        // if let Err(e) = self.cm.auth.invalidate(&account_id).await {
        //     tracing::error!(account_id = %account_id, error = %e, "Cache invalidation failed on account delete");
        // }

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use std::mem;
    use std::sync::Arc;

    use super::*;
    use crate::{
        app::{AppEnv, new_app_data},
        cache::{manager::CacheManager, mock::MockChx, redis::RedisChx},
        config::Config,
        core::services::registry::ServiceRegistry,
        create_dbx_mock_unsafe,
        dev::init::init_test,
        store::{
            ctx::StoreCtx,
            entities::{
                account::AccountRow,
                credential::{CredentialForCreate, CredentialProvider},
                workspace::WorkspaceRow,
            },
            error::StoreError,
            meta::StoreId,
            stores::{account::AccountStore, workspace::SYSTEM_CONST},
            traits::{contains::FilterByContains, crud::*, join::GetOneToMany},
        },
    };
    use anyhow::Result;
    use log::debug;
    use modql::filter::{ListOptions, OpValsString};
    use serde_json::json;
    use serial_test::serial;
    use std::any::TypeId;
    use uuid::Uuid;

    #[tokio::test]
    #[serial]
    async fn test_account_create() -> CoreResult<()> {
        let app = init_test().await;
        let acc_svc = app.svc_reg.account.clone();
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["account:create"])?;

        let system_ws = app
            .sm
            .workspace
            .get_by_slug_opt(&(&ctx).into(), SYSTEM_CONST.system_ws_slug)
            .await?
            .expect("system workspace not seeded");

        let mut params = AccountCreateParams::default();
        params.email = "new_exist@new.com".to_string();

        let new_acc = acc_svc.create(&mut ctx, params).await?;

        println!("{new_acc:?}");
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_account_list() -> CoreResult<()> {
        let app = init_test().await;
        let acc_svc = app.svc_reg.account.clone();
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["account:list"])?;

        let system_ws = app
            .sm
            .workspace
            .get_by_slug_opt(&(&ctx).into(), SYSTEM_CONST.system_ws_slug)
            .await?
            .expect("system workspace not seeded");

        let params = AccountListParams {
            filter: None,
            options: None,
        };

        let accounts = acc_svc.list(&mut ctx, params).await?;

        println!("{accounts:?}");
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_create_mock_account_success() -> CoreResult<()> {
        create_dbx_mock_unsafe!(
            MockDbxAccountRegister,
            fetch_one: {
                let acc = AccountRow::default();
                let result = unsafe { mem::transmute_copy::<AccountRow, O>(&acc) };
                mem::forget(acc);
                Ok(result)
            },
            fetch_optional: {
                let ws = WorkspaceRow::default();
                let result = unsafe { mem::transmute_copy::<WorkspaceRow, O>(&ws) };
                mem::forget(ws);
                Ok(Some(result))
             },
            fetch_all: { Ok(vec![]) },
            execute: { Ok(1) }
        );
        let config = Config::test_config();

        // build store manager
        let dbx = Arc::new(MockDbxAccountRegister);
        let sm = Arc::new(StoreManager::new(dbx));

        // build cache manager with mock (no real Redis connection needed)
        let mock_cache = Arc::new(MockChx::default());
        let cm = Arc::new(CacheManager::new(mock_cache));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.account.clone();
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["account:create"])?;

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
        create_dbx_mock_unsafe!(
            MockDbxAccountRegister,
            fetch_one: {
                // workspace lookup via fetch_one
                let ws = WorkspaceRow::default();
                let result = unsafe { mem::transmute_copy::<WorkspaceRow, O>(&ws) };
                mem::forget(ws);
                Ok(result)
            },
            fetch_optional: {
                // workspace lookup via fetch_one
                let ws = WorkspaceRow::default();
                let result = unsafe { mem::transmute_copy::<WorkspaceRow, O>(&ws) };
                mem::forget(ws);
                Ok(Some(result))
             },
            fetch_all: {
                let mut acc = AccountRow::default();
                acc.email = "user@user.com".to_string();
                let result = unsafe { mem::transmute_copy::<AccountRow, O>(&acc) };
                mem::forget(acc);
                Ok(vec![result])
            },
            execute: { Err(StoreError::MockReturn) }
        );

        let config = Config::test_config();

        // build store manager
        let dbx = Arc::new(MockDbxAccountRegister);
        let sm = Arc::new(StoreManager::new(dbx));

        // build cache manager with mock (no real Redis connection needed)
        let mock_cache = Arc::new(MockChx::default());
        let cm = Arc::new(CacheManager::new(mock_cache));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.account.clone();

        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["account:create"])?;

        let params = AccountCreateParams::default();
        let new_acc = svc.create(&mut ctx, params).await;

        // println!("new_acc: {:?}", new_acc);

        assert!(
            matches!(new_acc, Err(CoreError::AlreadyExists(..))),
            "should be CoreError::AlreadyExists"
        );

        Ok(())
    }
}
