use uuid::Uuid;

use crate::{
    core::models::{permission::PermissionCheck, workspace::Workspace},
    store::{
        entities::credential::{CredentialForCreate, CredentialForUpdate, CredentialRow},
        manager::StoreManager,
        stores::credential::CredentialStore,
        traits::{crud::*, dbx::DbExecutor},
    },
};
use crate::{
    core::{
        ctx::CoreCtx,
        error::CoreResult,
        models::{
            account::{Account, AccountDescribeParams},
            credential::{
                Credential, CredentialCreateParams, CredentialDeleteParams,
                CredentialDescribeParams, CredentialListParams, CredentialUpdateParams,
            },
            list::ListResponse,
        },
        services::{account::AccountService, auth::AuthValidator, workspace::WorkspaceService},
        traits::{
            list::RequestListParams,
            service::{
                CoreModelCreateService, CoreModelDeleteService, CoreModelDescribeService,
                CoreModelListService, CoreModelService, CoreModelUpdateService,
            },
        },
    },
    store::contains::FilterByContains,
};
use std::{collections::HashMap, sync::Arc};

pub struct CredentialService<D: DbExecutor> {
    sm: Arc<StoreManager<D>>,
    ws_svc: WorkspaceService<D>,
    acc_svc: AccountService<D>,
}

impl<D: DbExecutor> CredentialService<D> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        ws_svc: WorkspaceService<D>,
        acc_svc: AccountService<D>,
    ) -> Self {
        Self {
            sm,
            ws_svc,
            acc_svc,
        }
    }

    async fn get_account(
        &self,
        ctx: &mut CoreCtx,
        id: Uuid,
        workspace_id: Uuid,
    ) -> CoreResult<Account> {
        ctx.extend_perms(&["account:describe"])?;

        let account = self
            .acc_svc
            .describe(
                ctx,
                AccountDescribeParams {
                    workspace_id,
                    email: None,
                    id: Some(id),
                },
            )
            .await?;
        Ok(account)
    }

    async fn hydrate_credentials(
        &self,
        ctx: &mut CoreCtx,
        rows: Vec<CredentialRow>,
    ) -> CoreResult<Vec<Credential>> {
        let mut workspaces: HashMap<Uuid, Workspace> = HashMap::new();
        let mut accounts: HashMap<Uuid, Account> = HashMap::new();

        let mut credentials: Vec<Credential> = Vec::with_capacity(rows.len());

        // // Hydrate results
        for row in rows.into_iter() {
            let workspace_id: Uuid = row.workspace_id.into();
            let workspace = match workspaces.get(&workspace_id) {
                Some(ws) => ws,
                None => {
                    let ws = self.get_workspace(ctx, workspace_id).await?;
                    let ws_id = ws.id;
                    workspaces.insert(ws_id, ws);
                    // SAFETY: can unwrap as insert occurs directly above
                    workspaces.get(&ws_id).unwrap()
                }
            };
            let account_id: Uuid = row.account_id.into();
            let account = match accounts.get(&account_id) {
                Some(acc) => acc,
                None => {
                    let acc = self.get_account(ctx, account_id, workspace_id).await?;
                    let acc_id = acc.id;
                    accounts.insert(acc_id, acc);
                    // SAFETY: can unwrap as insert occurs directly above
                    accounts.get(&acc_id).unwrap()
                }
            };
            let project =
                Credential::from_row_with_entities(row, account.clone(), workspace.clone())?;
            credentials.push(project);
        }

        Ok(credentials)
    }
}

// --- Base Model Service ---
impl<D: DbExecutor> CoreModelService<D> for CredentialService<D> {
    type CoreModel = Credential;
    type ServiceStore = CredentialStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.credential
    }

    fn ws_svc(&self) -> &WorkspaceService<D> {
        &self.ws_svc
    }
}

// --- Create ---
impl<D: DbExecutor> CoreModelCreateService<D> for CredentialService<D> {
    type CreateParams = CredentialCreateParams;
    const CREATE_PERMISSION: &'static str = "credential:create";

    async fn create(
        &self,
        ctx: &mut CoreCtx,
        params: Self::CreateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        let for_create: CredentialForCreate = params.clone().into();

        let row = store.create(&store_ctx, for_create).await?;

        // // Use describe to return fully hydrated model
        self.describe(
            ctx,
            CredentialDescribeParams {
                id: row.id.into(),
                account_id: params.account_id,
                workspace_id: params.workspace_id,
                provider_id: None,
                email: None,
            },
        )
        .await
    }
}

// --- Describe (with Hydration) ---
impl<D: DbExecutor> CoreModelDescribeService<D> for CredentialService<D> {
    type DescribeParams = CredentialDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = "credential:describe";

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DescribeParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
            .await?;

        let row = store.get(&store_ctx, &params.id.into()).await?;

        let acc = self
            .get_account(ctx, params.account_id, params.workspace_id)
            .await?;
        let ws = self.get_workspace(ctx, params.workspace_id).await?;

        let credential = Credential::from_row_with_entities(row, acc, ws)?;

        Ok(credential)
    }
}

// --- List ---
impl<D: DbExecutor> CoreModelListService<D> for CredentialService<D> {
    type ListParams = CredentialListParams;
    const LIST_PERMISSION: &'static str = "credential:list";

    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: Self::ListParams,
    ) -> CoreResult<ListResponse<Self::CoreModel>> {
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::LIST_PERMISSION])
            .await?;
        // validate params
        let list_options = params.list_options();
        let tags_filter = params.validate_filter_tags()?;

        if let Some(tags) = tags_filter.tags() {
            let data = store
                .filter_by_tags_contain(&store_ctx, tags.clone(), Some(list_options.clone()))
                .await?;
            let total = store.count_by_tags_contain(&store_ctx, tags).await?;

            let projects = self.hydrate_credentials(ctx, data).await?;

            return Ok(ListResponse::new(projects, total, list_options));
        }

        if let Some(filter) = tags_filter.filter() {
            let data = store
                .list(&store_ctx, Some(filter.clone()), Some(list_options.clone()))
                .await?;
            let total = store.count(&store_ctx, Some(filter)).await?;

            let projects = self.hydrate_credentials(ctx, data).await?;

            return Ok(ListResponse::new(projects, total, list_options));
        }

        // empty response
        Ok(ListResponse::default())
    }
}

// --- Update ---
impl<D: DbExecutor> CoreModelUpdateService<D> for CredentialService<D> {
    type UpdateParams = CredentialUpdateParams;
    const UPDATE_PERMISSION: &'static str = "credential:update";

    async fn update(
        &self,
        ctx: &mut CoreCtx,
        params: Self::UpdateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::UPDATE_PERMISSION])
            .await?;

        let for_update: CredentialForUpdate = params.clone().into();

        store
            .update(&store_ctx, &params.id.into(), for_update)
            .await?;

        self.describe(
            ctx,
            CredentialDescribeParams {
                id: params.id,
                account_id: params.account_id,
                workspace_id: params.workspace_id,
                provider_id: None,
                email: None,
            },
        )
        .await
    }
}

// --- Delete ---
impl<D: DbExecutor> CoreModelDeleteService<D> for CredentialService<D> {
    type DeleteParams = CredentialDeleteParams;
    const DELETE_PERMISSION: &'static str = "credential:delete";

    async fn delete(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DeleteParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
            .await?;

        let to_delete = self
            .describe(
                ctx,
                CredentialDescribeParams {
                    id: params.id,
                    account_id: params.account_id,
                    workspace_id: params.workspace_id,
                    provider_id: None,
                    email: None,
                },
            )
            .await?;

        let res = store.delete(&store_ctx, &params.id.into()).await?;

        Ok(to_delete)
    }
}
