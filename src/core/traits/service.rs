use uuid::Uuid;

use crate::{
    cache::{entities::workspace::WorkspaceCache, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::CoreResult,
        models::{
            list::ListResponse,
            permission::PermissionRule,
            workspace::Workspace,
        },
        services::{
            auth::AuthValidator, permission::CANONICAL_PERMISSIONS, workspace::WorkspaceService,
        },
    },
    store::{ctx::StoreCtx, traits::dbx::DbExecutor},
};

pub trait CoreModelService<D: DbExecutor, C: CacheExecutor> {
    type CoreModel;
    type ServiceStore;

    fn store(&self) -> &Self::ServiceStore;
    fn ws_svc(&self) -> &WorkspaceService<D, C>;

    async fn get_workspace(
        &self,
        ctx: &mut CoreCtx,
        workspace_id: Uuid,
    ) -> CoreResult<WorkspaceCache> {
        ctx.extend_perms(&[CANONICAL_PERMISSIONS.workspace.describe])?;

        let ws = self
            .ws_svc()
            .get_and_cache(ctx, &workspace_id.to_string())
            .await?;

        Ok(ws.into())
    }

    fn should_remove_workspace_from_store_ctx(&self) -> bool {
        false
    }

    async fn scope_and_validate_ctx(
        &self,
        ctx: &mut CoreCtx,
        workspace_id: Uuid,
        required_perms: &[&str],
    ) -> CoreResult<(StoreCtx, WorkspaceCache)> {
        let workspace = self.get_workspace(ctx, workspace_id).await?;

        let auth_validator = AuthValidator::new(ctx);

        // validate permissions
        auth_validator.validate_ctx_perms(required_perms)?;

        // scope store_ctx
        let mut store_ctx = auth_validator.scope_store_workspace(Some(workspace.id))?;

        if (self.should_remove_workspace_from_store_ctx()) {
            store_ctx.set_workspace_scope(None);
        }

        Ok((store_ctx, workspace))
    }
}

pub trait CoreModelCreateService<D: DbExecutor, C: CacheExecutor>: CoreModelService<D, C> {
    type CreateParams;
    const CREATE_PERMISSION: &'static str;

    async fn create(
        &self,
        ctx: &mut CoreCtx,
        params: Self::CreateParams,
    ) -> CoreResult<Self::CoreModel>;
}

pub trait CoreModelDescribeService<D: DbExecutor, C: CacheExecutor>:
    CoreModelService<D, C>
{
    type DescribeParams;
    const DESCRIBE_PERMISSION: &'static str;

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DescribeParams,
    ) -> CoreResult<Self::CoreModel>;
}

pub trait CoreModelListService<D: DbExecutor, C: CacheExecutor>: CoreModelService<D, C> {
    type ListParams;
    const LIST_PERMISSION: &'static str;

    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: Self::ListParams,
    ) -> CoreResult<ListResponse<Self::CoreModel>>;
}

pub trait CoreModelUpdateService<D: DbExecutor, C: CacheExecutor>: CoreModelService<D, C> {
    type UpdateParams;
    const UPDATE_PERMISSION: &'static str;

    async fn update(
        &self,
        ctx: &mut CoreCtx,
        params: Self::UpdateParams,
    ) -> CoreResult<Self::CoreModel>;
}

pub trait CoreModelDeleteService<D: DbExecutor, C: CacheExecutor>: CoreModelService<D, C> {
    type DeleteParams;
    const DELETE_PERMISSION: &'static str;

    async fn delete(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DeleteParams,
    ) -> CoreResult<Self::CoreModel>;
}

/// A convenience trait that groups all CRUD operations.
pub trait CoreModelCrudService<D: DbExecutor, C: CacheExecutor>:
    CoreModelCreateService<D, C>
    + CoreModelDescribeService<D, C>
    + CoreModelListService<D, C>
    + CoreModelUpdateService<D, C>
    + CoreModelDeleteService<D, C>
{
}

// Blanket implementation for any service that meets all criteria
impl<T, D: DbExecutor, C: CacheExecutor> CoreModelCrudService<D, C> for T where
    T: CoreModelCreateService<D, C>
        + CoreModelDescribeService<D, C>
        + CoreModelListService<D, C>
        + CoreModelUpdateService<D, C>
        + CoreModelDeleteService<D, C>
{
}
