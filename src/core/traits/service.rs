use uuid::Uuid;

use crate::{
    core::{
        ctx::CoreCtx,
        error::CoreResult,
        models::{
            list::ListResponse,
            permission::PermissionCheck,
            workspace::{Workspace, WorkspaceDescribeParams},
        },
        services::{auth::AuthValidator, workspace::WorkspaceService},
    },
    store::{ctx::StoreCtx, traits::dbx::DbExecutor},
};

pub trait CoreModelService<D: DbExecutor> {
    type CoreModel;
    type ServiceStore;

    fn store(&self) -> &Self::ServiceStore;
    fn ws_svc(&self) -> &WorkspaceService<D>;

    async fn get_workspace(&self, ctx: &mut CoreCtx, workspace_id: Uuid) -> CoreResult<Workspace> {
        let params = WorkspaceDescribeParams {
            id: Some(workspace_id),
            slug: None,
        };

        ctx.extend_perms(&["workspace:describe"])?;

        self.ws_svc().describe(ctx, params).await
    }

    fn should_remove_workspace_from_store_ctx(&self) -> bool {
        false
    }

    async fn scope_and_validate_ctx(
        &self,
        ctx: &mut CoreCtx,
        workspace_id: Uuid,
        required_perms: &[&str],
    ) -> CoreResult<(StoreCtx, Workspace)> {
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

pub trait CoreModelCreateService<D: DbExecutor>: CoreModelService<D> {
    type CreateParams;
    const CREATE_PERMISSION: &'static str;

    async fn create(
        &self,
        ctx: &mut CoreCtx,
        params: Self::CreateParams,
    ) -> CoreResult<Self::CoreModel>;
}

pub trait CoreModelDescribeService<D: DbExecutor>: CoreModelService<D> {
    type DescribeParams;
    const DESCRIBE_PERMISSION: &'static str;

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DescribeParams,
    ) -> CoreResult<Self::CoreModel>;
}

pub trait CoreModelListService<D: DbExecutor>: CoreModelService<D> {
    type ListParams;
    const LIST_PERMISSION: &'static str;

    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: Self::ListParams,
    ) -> CoreResult<ListResponse<Self::CoreModel>>;
}

pub trait CoreModelUpdateService<D: DbExecutor>: CoreModelService<D> {
    type UpdateParams;
    const UPDATE_PERMISSION: &'static str;

    async fn update(
        &self,
        ctx: &mut CoreCtx,
        params: Self::UpdateParams,
    ) -> CoreResult<Self::CoreModel>;
}

pub trait CoreModelDeleteService<D: DbExecutor>: CoreModelService<D> {
    type DeleteParams;
    const DELETE_PERMISSION: &'static str;

    async fn delete(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DeleteParams,
    ) -> CoreResult<Self::CoreModel>;
}

/// A convenience trait that groups all CRUD operations.
pub trait CoreModelCrudService<D: DbExecutor>:
    CoreModelCreateService<D>
    + CoreModelDescribeService<D>
    + CoreModelListService<D>
    + CoreModelUpdateService<D>
    + CoreModelDeleteService<D>
{
}

// Blanket implementation for any service that meets all criteria
impl<T, D: DbExecutor> CoreModelCrudService<D> for T where
    T: CoreModelCreateService<D>
        + CoreModelDescribeService<D>
        + CoreModelListService<D>
        + CoreModelUpdateService<D>
        + CoreModelDeleteService<D>
{
}
