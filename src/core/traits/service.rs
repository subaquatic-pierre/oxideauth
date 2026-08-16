use uuid::Uuid;

use crate::{
    cache::{entities::workspace::WorkspaceCache, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            list::ListResponse, workspace::WorkspaceDescribeParams,
        },
        services::{validator::AuthValidator, workspace::WorkspaceService},
    },
    store::{ctx::StoreCtx, traits::dbx::DbExecutor},
};

pub trait CoreModelService<D: DbExecutor, C: CacheExecutor> {
    type CoreModel;
    type ServiceStore;

    fn store(&self) -> &Self::ServiceStore;
    fn ws_svc(&self) -> &WorkspaceService<D, C>;
    fn validator(&self) -> &AuthValidator;

    /// Whether the service operates on workspace-scoped data.
    ///
    /// A scoped service MUST receive a concrete `workspace_id` when calling
    /// [`scope_and_validate`]; this is enforced as a safe guard there.
    fn is_scoped(&self) -> bool;

    async fn get_workspace(
        &self,
        ctx: &mut CoreCtx,
        workspace_id: Uuid,
    ) -> CoreResult<WorkspaceCache> {
        let ws = self
            .ws_svc()
            .get_and_cache(
                ctx,
                &WorkspaceDescribeParams {
                    id: Some(workspace_id),
                    slug: None,
                },
            )
            .await?;

        Ok(ws.into())
    }

    /// Validates `required_perms` and builds a store context.
    ///
    /// For a workspace-scoped service ([`is_scoped`] == `true`), `workspace_id`
    /// MUST be `Some` (a `None` is rejected here). For a global-table service
    /// ([`is_scoped`] == `false`), pass `None` to query unscoped; a `Some` value
    /// may still be supplied to scope by a specific workspace (e.g. a
    /// namespace-scoped listing on a global table).
    ///
    /// Returns only the newly-scoped [`StoreCtx`]; callers that need the
    /// workspace read it directly off [`CoreCtx`] (e.g. `ctx.ws_cache`).
    async fn scope_and_validate(
        &self,
        ctx: &mut CoreCtx,
        workspace_id: Option<Uuid>,
        required_perms: &[&str],
    ) -> CoreResult<StoreCtx> {
        let auth_validator = self.validator();

        // validate permissions
        auth_validator.validate_ctx_perms(ctx, required_perms)?;

        // Safe guard: a workspace-scoped service must receive a concrete workspace.
        if self.is_scoped() && workspace_id.is_none() {
            return Err(CoreError::InvalidParams(
                "workspace_id is required for workspace-scoped services".to_string(),
            ));
        }

        let store_ctx = match workspace_id {
            Some(ws_id) => {
                // NOTE(workspace-scope): scoped — scoped to the requested workspace.
                auth_validator.scope_store_workspace(ctx, Some(ws_id))?
            }
            None => {
                // NOTE(workspace-scope): unscoped — global table (no workspace_id column).
                let mut store_ctx: StoreCtx = ctx.into();
                store_ctx.set_workspace_scope(None);
                store_ctx
            }
        };

        Ok(store_ctx)
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
