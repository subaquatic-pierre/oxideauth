use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use crate::{
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::Account,
            list::ListResponse,
            workspace::{
                Workspace, WorkspaceCreateParams, WorkspaceDeleteParams, WorkspaceDescribeParams,
                WorkspaceListParams, WorkspaceUpdateParams,
            },
        },
        services::auth::AuthValidator,
        traits::{
            list::RequestListParams,
            service::{
                CoreModelCreateService, CoreModelDeleteService, CoreModelDescribeService,
                CoreModelListService, CoreModelService, CoreModelUpdateService,
            },
        },
    },
    store::{
        ctx::StoreCtx,
        dbx::PgDbx,
        entities::{
            account::{AccountFilter, AccountForCreate, AccountMeta},
            id::DbId,
            workspace::{
                WorkspaceConfig as StoreWorkspaceConfig, WorkspaceFilter, WorkspaceForCreate,
                WorkspaceForUpdate,
            },
        },
        error::StoreError,
        manager::StoreManager,
        stores::workspace::WorkspaceStore,
        traits::{contains::FilterByContains, crud::*, dbx::DbExecutor},
        utils::ListOptionsValidator,
    },
};

pub struct WorkspaceService<D: DbExecutor> {
    sm: Arc<StoreManager<D>>,
}

impl<D: DbExecutor> WorkspaceService<D> {
    pub fn new(sm: Arc<StoreManager<D>>) -> Self {
        Self { sm }
    }

    async fn get_workspace_id(
        &self,
        ctx: &CoreCtx,
        id: Option<Uuid>,
        slug: Option<String>,
    ) -> CoreResult<Uuid> {
        let store = self.store();

        let id: DbId = match (id, slug) {
            (Some(id), _) => id.into(),
            (None, Some(slug)) => match store.get_by_slug_opt(&ctx.into(), &slug).await? {
                Some(ws) => ws.id,
                None => {
                    return Err(CoreError::StoreError(StoreError::EntityNotFound {
                        entity: "workspace".to_string(),
                        id: slug.to_string(),
                    }))
                }
            },
            (None, None) => {
                return Err(CoreError::InvalidParams(
                    "Workspace ID or slug required".to_string(),
                ))
            }
        };

        Ok(id.into())
    }
}

impl<D: DbExecutor> CoreModelService<D> for WorkspaceService<D> {
    type CoreModel = Workspace;

    type ServiceStore = WorkspaceStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.workspace
    }

    fn ws_svc(&self) -> &WorkspaceService<D> {
        &self
    }

    fn should_remove_workspace_from_store_ctx(&self) -> bool {
        true
    }
}

impl<D: DbExecutor> CoreModelCreateService<D> for WorkspaceService<D> {
    type CreateParams = WorkspaceCreateParams;
    const CREATE_PERMISSION: &'static str = "workspace:create";

    /// Creates a new workspace.
    async fn create(
        &self,
        ctx: &mut CoreCtx,
        params: WorkspaceCreateParams,
    ) -> CoreResult<Workspace> {
        let store = self.store();

        let auth_validator = self.validator(&ctx);

        // validate permissions
        auth_validator.validate_ctx_perms(&[Self::CREATE_PERMISSION])?;

        // scope store_ctx
        let store_ctx = auth_validator.scope_store_workspace(None)?;

        // 1. Check if slug already exists
        if store
            .get_by_slug_opt(&store_ctx, &params.slug)
            .await?
            .is_some()
        {
            return Err(CoreError::AlreadyExists(
                "Workspace slug already exists".to_string(),
            ));
        }

        let config = StoreWorkspaceConfig::default();

        // 2. Map Core Params to Store ForCreate struct
        let n_ws = WorkspaceForCreate {
            name: params.name,
            slug: params.slug,
            description: params.description,
            config: config,
            tags: params.tags,
            meta: params.meta,
        };

        // 3. Execute store creation
        let new_workspace = store.create(&store_ctx, n_ws).await?;

        Ok(new_workspace.into())
    }
}

impl<D: DbExecutor> CoreModelDescribeService<D> for WorkspaceService<D> {
    type DescribeParams = WorkspaceDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = "workspace:describe";

    /// Retrieves a single workspace by ID or slug.
    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: WorkspaceDescribeParams,
    ) -> CoreResult<Workspace> {
        let store = self.store();

        let workspace_id = self.get_workspace_id(ctx, params.id, params.slug).await?;

        let auth_validator = self.validator(&ctx);

        // validate permissions
        auth_validator.validate_ctx_perms(&[Self::DESCRIBE_PERMISSION])?;

        // scope store_ctx
        let store_ctx = auth_validator.scope_store_workspace(None)?;

        let res = store.get(&store_ctx, &workspace_id.into()).await?;

        Ok(res.into())
    }
}

impl<D: DbExecutor> CoreModelListService<D> for WorkspaceService<D> {
    type ListParams = WorkspaceListParams;
    const LIST_PERMISSION: &'static str = "workspace:list";

    /// Lists workspaces based on filter and options.
    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: WorkspaceListParams,
    ) -> CoreResult<ListResponse<Workspace>> {
        let store = self.store();
        let auth_validator = self.validator(&ctx);

        // validate permissions
        auth_validator.validate_ctx_perms(&[Self::LIST_PERMISSION])?;

        // scope store_ctx
        let store_ctx = auth_validator.scope_store_workspace(None)?;

        let options = params.list_options();

        let tags_filter = params.validate_filter_tags()?;

        // filter by tags
        if let Some(tags) = tags_filter.tags() {
            let data = store
                .filter_by_tags_contain(&store_ctx, tags.clone(), Some(options.clone()))
                .await?;
            let total = store.count_by_tags_contain(&store_ctx, tags).await?;

            let accounts: Vec<Workspace> = data.into_iter().map(|el| el.into()).collect();
            return Ok(ListResponse::new(accounts, total, options));
        }

        // filter by filter
        if let Some(filter) = tags_filter.filter() {
            let filter = Some(filter);
            let data = store
                .list(&store_ctx, filter.clone(), Some(options.clone()))
                .await?;
            let total = store.count(&store_ctx, filter).await?;
            let accounts: Vec<Workspace> = data.into_iter().map(|el| el.into()).collect();
            return Ok(ListResponse::new(accounts, total, options));
        }

        // empty result
        Ok(ListResponse::default())
    }
}

impl<D: DbExecutor> CoreModelUpdateService<D> for WorkspaceService<D> {
    type UpdateParams = WorkspaceUpdateParams;
    const UPDATE_PERMISSION: &'static str = "workspace:update";

    /// Updates an existing workspace.
    async fn update(
        &self,
        ctx: &mut CoreCtx,
        params: WorkspaceUpdateParams,
    ) -> CoreResult<Workspace> {
        let store = self.store();

        let auth_validator = self.validator(&ctx);

        // validate permissions
        auth_validator.validate_ctx_perms(&[Self::UPDATE_PERMISSION])?;

        // scope store_ctx
        let store_ctx = auth_validator.scope_store_workspace(None)?;

        let id = self
            .get_workspace_id(ctx, params.id, params.slug.clone())
            .await?;

        let config = StoreWorkspaceConfig::default();

        let update_data = WorkspaceForUpdate {
            name: params.name,
            slug: params.slug,
            description: params.description,
            config: Some(config),
            tags: params.tags,
            meta: params.meta,
        };

        let res = store.update(&store_ctx, &id.into(), update_data).await?;

        Ok(res.into())
    }
}

impl<D: DbExecutor> CoreModelDeleteService<D> for WorkspaceService<D> {
    type DeleteParams = WorkspaceDeleteParams;
    const DELETE_PERMISSION: &'static str = "workspace:delete";

    /// Deletes a workspace by ID or slug.
    async fn delete(
        &self,
        ctx: &mut CoreCtx,
        params: WorkspaceDeleteParams,
    ) -> CoreResult<Workspace> {
        // Returns the ID of the deleted item
        let store = self.store();

        let auth_validator = self.validator(&ctx);

        // validate permissions
        auth_validator.validate_ctx_perms(&[Self::DELETE_PERMISSION])?;

        // scope store_ctx
        let store_ctx = auth_validator.scope_store_workspace(None)?;

        let id = self.get_workspace_id(ctx, params.id, params.slug).await?;

        let deleted = store.delete(&store_ctx, &id.into()).await?;

        Ok(deleted.into())
    }
}

#[cfg(test)]
mod tests {
    use std::mem;
    use std::sync::Arc;

    use super::*;
    use crate::{
        core::models::list::RequestFilterParams,
        create_dbx_mock_unsafe,
        dev::{fixtures::global_ws_id, init::init_test},
        store::{
            ctx::StoreCtx,
            entities::{
                account::AccountRow,
                credential::{CredentialForCreate, CredentialProvider},
            },
            error::StoreError,
            meta::StoreId,
            stores::workspace::WorkspaceStore,
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
    async fn test_workspace_describe() -> CoreResult<()> {
        let app = init_test().await;
        let svc = app.svc_factory.workspace();
        let mut ctx = CoreCtx::new_test()?;

        let mut params = WorkspaceDescribeParams::default();
        params.id = Some(global_ws_id());

        let err = svc.describe(&mut ctx, params.clone()).await;

        ctx.extend_perms(&["workspace:describe"])?;

        let ok = svc.describe(&mut ctx, params).await;

        assert!(matches!(ok, Ok(..)));
        assert!(matches!(err, Err(..)));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_workspace_create() -> CoreResult<()> {
        let app = init_test().await;
        let svc = app.svc_factory.workspace();
        let mut ctx = CoreCtx::new_test()?;

        let slug = format!("test-ws-{}", Uuid::new_v4());
        let mut params = WorkspaceCreateParams::default();
        params.slug = slug.clone();
        params.name = "Test Workspace".to_string();

        // 1. Test unauthorized (missing permission)
        let err = svc.create(&mut ctx, params.clone()).await;
        assert!(
            err.is_err(),
            "Should fail without workspace:create permission"
        );

        // 2. Test success
        ctx.extend_perms(&["workspace:create"])?;
        let workspace = svc.create(&mut ctx, params.clone()).await?;

        assert_eq!(workspace.name, "Test Workspace");
        assert_eq!(workspace.slug, slug);

        // 3. Test duplicate slug
        let err_dup = svc.create(&mut ctx, params).await;
        assert!(matches!(err_dup, Err(CoreError::AlreadyExists(_))));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_workspace_list() -> CoreResult<()> {
        let app = init_test().await;
        let svc = app.svc_factory.workspace();
        let mut ctx = CoreCtx::new_test()?;
        ctx.extend_perms(&["workspace:create"])?;

        let mut params = WorkspaceCreateParams::default();
        params.name = "list-ns-a".to_string();
        params.slug = "list-ns-a-test-a".to_string();
        params.description = Some("LIST_BY_DESCRIPTION".to_string());
        let workspace = svc.create(&mut ctx, params.clone()).await?;
        params.name = "list-ns-b".to_string();
        params.slug = "list-ns-a-test-b".to_string();
        let workspace = svc.create(&mut ctx, params.clone()).await?;

        ctx.extend_perms(&["workspace:list"])?;

        let filter: WorkspaceFilter = json!({ "description": "LIST_BY_DESCRIPTION" }).try_into()?;
        let filter = RequestFilterParams {
            fields: Some(filter),
            tags: None,
        };
        let options = ListOptions::from_limit(1);

        let params = WorkspaceListParams {
            filter: Some(filter),
            options: Some(options),
        };

        let res = svc.list(&mut ctx, params).await?;

        assert!(res.data.len() == 1);
        assert!(res.metadata.total >= 1);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_workspace_update() -> CoreResult<()> {
        let app = init_test().await;
        let svc = app.svc_factory.workspace();
        let mut ctx = CoreCtx::new_test()?;

        // Setup: Create a workspace to update
        ctx.extend_perms(&["workspace:update", "workspace:create"])?;
        let slug = format!("update-me-{}", Uuid::new_v4());
        let created = svc
            .create(
                &mut ctx,
                WorkspaceCreateParams {
                    name: "Original Name".to_string(),
                    slug,
                    ..Default::default()
                },
            )
            .await?;

        // Update name
        let update_params = WorkspaceUpdateParams {
            id: Some(created.id),
            name: Some("Updated Name".to_string()),
            ..Default::default()
        };

        let updated = svc.update(&mut ctx, update_params).await?;
        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.id, created.id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_workspace_delete() -> CoreResult<()> {
        let app = init_test().await;
        let svc = app.svc_factory.workspace();
        let mut ctx = CoreCtx::new_test()?;

        ctx.extend_perms(&["workspace:create", "workspace:delete", "workspace:describe"])?;

        // Setup: Create workspace
        let slug = format!("delete-me-{}", Uuid::new_v4());
        let created = svc
            .create(
                &mut ctx,
                WorkspaceCreateParams {
                    name: "To Be Deleted".to_string(),
                    slug: slug.clone(),
                    ..Default::default()
                },
            )
            .await?;

        // Delete
        let delete_params = WorkspaceDeleteParams {
            id: Some(created.id),
            slug: None,
        };
        let deleted = svc.delete(&mut ctx, delete_params).await?;
        assert_eq!(deleted.id, created.id);

        // Verify it's gone
        let desc_params = WorkspaceDescribeParams {
            id: Some(created.id),
            slug: None,
        };
        let err = svc.describe(&mut ctx, desc_params).await;
        assert!(err.is_err());

        Ok(())
    }
}
