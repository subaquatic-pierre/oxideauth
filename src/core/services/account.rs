use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use crate::{
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::{
                Account, AccountCreateParams, AccountDeleteParams, AccountDescribeParams,
                AccountListParams, AccountUpdateParams,
            },
            list::{ListResponse, ListResponseMeta},
            permission::PermissionCheck,
            workspace::{Workspace, WorkspaceDescribeParams},
        },
        services::{auth::AuthValidator, workspace::WorkspaceService},
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
            account::{AccountFilter, AccountForCreate, AccountForUpdate, AccountMeta},
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

pub struct AccountService<D: DbExecutor> {
    sm: Arc<StoreManager<D>>,
    ws_svc: WorkspaceService<D>,
}

impl<D: DbExecutor> CoreModelService<D> for AccountService<D> {
    type CoreModel = Account;
    type ServiceStore = AccountStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.account
    }

    fn ws_svc(&self) -> &WorkspaceService<D> {
        &self.ws_svc
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

impl<D: DbExecutor> AccountService<D> {
    pub fn new(sm: Arc<StoreManager<D>>, ws_svc: WorkspaceService<D>) -> Self {
        Self { sm, ws_svc }
    }

    async fn get_account_id(
        &self,
        store_ctx: &StoreCtx,
        id: Option<Uuid>,
        email: Option<String>,
    ) -> CoreResult<DbId> {
        let store = self.store();

        let id: DbId = match (id, email) {
            (Some(id), _) => id.into(),
            (None, Some(email)) => match store.get_by_email(store_ctx, &email).await? {
                Some(acc) => acc.id,
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
}

impl<D: DbExecutor> CoreModelCreateService<D> for AccountService<D> {
    type CreateParams = AccountCreateParams;
    const CREATE_PERMISSION: &'static str = "account:create";

    async fn create(&self, ctx: &mut CoreCtx, params: AccountCreateParams) -> CoreResult<Account> {
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        if store
            .get_by_email(&store_ctx, &params.email)
            .await?
            .is_some()
        {
            return Err(CoreError::AlreadyExists("email already exists".to_string()));
        }

        let n_acc = AccountForCreate {
            email: params.email,
            name: "name".to_string(),
            description: None,
            avatar_url: None,
            enabled: false,
            verified: false,
            tags: vec![],
            meta: AccountMeta {
                schema_version: "1".to_string(),
            },
        };

        let new_account = store.create(&store_ctx, n_acc).await?;

        Ok(new_account.into())
    }
}
impl<D: DbExecutor> CoreModelDescribeService<D> for AccountService<D> {
    type DescribeParams = AccountDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = "account:describe";

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: AccountDescribeParams,
    ) -> CoreResult<Account> {
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
            .await?;

        let id = self
            .get_account_id(&store_ctx, params.id, params.email)
            .await?;

        let acc: Account = store.get(&store_ctx, &id).await?.into();

        Ok(acc)
    }
}

impl<D: DbExecutor> CoreModelListService<D> for AccountService<D> {
    type ListParams = AccountListParams;
    const LIST_PERMISSION: &'static str = "account:list";

    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: AccountListParams,
    ) -> CoreResult<ListResponse<Account>> {
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::LIST_PERMISSION])
            .await?;

        let options = params.list_options();

        let tags_filter = params.validate_filter_tags()?;

        // NOTE: Account can never be workspace scoped, like other services such as ProjectService, because the model does not have a workspace_id field on it. This means Accounts are always global scoped. We have to find a different way to scope accounts by workspace, or only reserve account::list permission to memberships in the global namespace

        // filter by tags
        if let Some(tags) = tags_filter.tags() {
            let data = store
                .filter_by_tags_contain(&store_ctx, tags.clone(), Some(options.clone()))
                .await?;
            let total = store.count_by_tags_contain(&store_ctx, tags).await?;

            let accounts: Vec<Account> = data.into_iter().map(|el| el.into()).collect();
            return Ok(ListResponse::new(accounts, total, options));
        }

        // filter by filter
        if let Some(filter) = tags_filter.filter() {
            let filter = Some(filter);
            let data = store
                .list(&store_ctx, filter.clone(), Some(options.clone()))
                .await?;
            let total = store.count(&store_ctx, filter).await?;
            let accounts: Vec<Account> = data.into_iter().map(|el| el.into()).collect();
            return Ok(ListResponse::new(accounts, total, options));
        }

        // empty result
        Ok(ListResponse::default())
    }
}

impl<D: DbExecutor> CoreModelUpdateService<D> for AccountService<D> {
    type UpdateParams = AccountUpdateParams;
    const UPDATE_PERMISSION: &'static str = "account:update";

    async fn update(&self, ctx: &mut CoreCtx, params: AccountUpdateParams) -> CoreResult<Account> {
        let store = self.store();
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::UPDATE_PERMISSION])
            .await?;

        let email = params.email.clone();
        let id = self
            .get_account_id(&store_ctx, params.id, params.email)
            .await?;

        // TODO: updating email constraints need to be enforced
        // if email is updated then need to set verified as false
        // ensure email does not already exist for a different account
        // force reverify
        // this email acts primarily as ID, if a user wants to update email
        // to login then can update credential used for that namespace instead

        // Prepare the update struct for the store layer
        let update_data = AccountForUpdate {
            email: None, // NOTE: read above for decision
            name: params.name,
            description: params.description,
            avatar_url: params.avatar_url,
            enabled: params.enabled,
            verified: params.verified,
            tags: params.tags,
            meta: params.meta,
        };

        let updated_account = store.update(&store_ctx, &id, update_data).await?;

        Ok(updated_account.into())
    }
}

impl<D: DbExecutor> CoreModelDeleteService<D> for AccountService<D> {
    type DeleteParams = AccountDeleteParams;
    const DELETE_PERMISSION: &'static str = "account:delete";

    async fn delete(&self, ctx: &mut CoreCtx, params: AccountDeleteParams) -> CoreResult<Account> {
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
            .await?;

        let id = self
            .get_account_id(&store_ctx, params.id, params.email)
            .await?;

        let deleted = store.delete(&store_ctx, &id).await?.into();

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
        cache::{manager::CacheManager, redis::RedisChx},
        config::Config,
        core::services::factory::ServiceFactory,
        create_dbx_mock_unsafe,
        dev::{
            fixtures::{global_ws_id, root_user_id},
            init::init_test,
        },
        store::{
            ctx::StoreCtx,
            entities::{
                account::AccountRow,
                credential::{CredentialForCreate, CredentialProvider},
                workspace::WorkspaceRow,
            },
            error::StoreError,
            meta::StoreId,
            stores::account::AccountStore,
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
        let acc_svc = app.svc_factory.account();
        let mut ctx = CoreCtx::new_test()?;
        ctx.extend_perms(&["account:create"])?;

        let mut params = AccountCreateParams::default();
        params.workspace_id = global_ws_id();
        params.email = "new_exist@new.com".to_string();

        let new_acc = acc_svc.create(&mut ctx, params).await?;

        println!("{new_acc:?}");
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_account_list() -> CoreResult<()> {
        let app = init_test().await;
        let acc_svc = app.svc_factory.account();
        let mut ctx = CoreCtx::new_test()?;
        ctx.extend_perms(&["account:list"])?;

        let params = AccountListParams {
            workspace_id: global_ws_id(),
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

        // build cache manager
        let redis_cache = Arc::new(RedisChx::new(&config.redis_url).await);
        let cm = Arc::new(CacheManager::new(redis_cache));
        let svc_factory = ServiceFactory::new(sm, cm);
        let svc = svc_factory.account();
        let mut ctx = CoreCtx::new_test()?;
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

        // build cache manager
        let redis_cache = Arc::new(RedisChx::new(&config.redis_url).await);
        let cm = Arc::new(CacheManager::new(redis_cache));
        let svc_factory = ServiceFactory::new(sm, cm);
        let svc = svc_factory.account();

        let mut ctx = CoreCtx::new_test()?;
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
