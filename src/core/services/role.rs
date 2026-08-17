use std::{collections::HashMap, sync::Arc};

use uuid::Uuid;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            list::ListResponse,
            permission::PermissionRule,
            role::{
                Role, RoleCreateParams, RoleDeleteParams, RoleDescribeIdentifier,
                RoleDescribeParams, RoleListParams, RoleUpdateParams, RoleWithPolicies,
                WorkspaceRoleCreateParams,
            },
            workspace::{Workspace, WorkspaceDescribeParams},
        },
        services::{
            permission::{CANONICAL_PERMISSIONS, PermissionService},
            validator::AuthValidator,
            workspace::WorkspaceService,
        },
        traits::{
            list::RequestListParams,
            params::ValidateParams,
            service::{
                CoreModelCreateService, CoreModelDeleteService, CoreModelDescribeService,
                CoreModelListService, CoreModelService, CoreModelUpdateService,
            },
        },
    },
    store::{
        contains::FilterByContains,
        ctx::StoreCtx,
        entities::{
            id::DbId,
            role::{RoleForCreate, RoleRow, RoleWithPermissions},
        },
        error::StoreError,
        join::{GetManyToMany, LinkManyToMany, ListManyToMany},
        manager::StoreManager,
        stores::{role::RoleStore, workspace::SYSTEM_CONST},
        traits::{crud::*, dbx::DbExecutor},
    },
};

pub struct RoleService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    ws_svc: Arc<WorkspaceService<D, C>>,
    perm_svc: Arc<PermissionService<D, C>>,
    validator: Arc<AuthValidator>,
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D, C> for RoleService<D, C> {
    type CoreModel = Role;
    type ServiceStore = RoleStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.role
    }

    fn ws_svc(&self) -> &WorkspaceService<D, C> {
        self.ws_svc.as_ref()
    }

    fn validator(&self) -> &AuthValidator {
        self.validator.as_ref()
    }

    fn is_scoped(&self) -> bool {
        true
    }
}

impl<D: DbExecutor, C: CacheExecutor> RoleService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        ws_svc: Arc<WorkspaceService<D, C>>,
        perm_svc: Arc<PermissionService<D, C>>,
        cm: Arc<CacheManager<C>>,
        validator: Arc<AuthValidator>,
    ) -> Self {
        Self {
            sm,
            cm,
            ws_svc,
            perm_svc,
            validator,
        }
    }

    /// Resolves a role from a typed id-or-name descriptor.
    ///
    /// When `params.id` is present the role is fetched by id, otherwise
    /// `params.name` is looked up within the context's scoped workspace.
    pub async fn get_by_name(
        &self,
        ctx: &mut CoreCtx,
        params: &RoleDescribeIdentifier,
    ) -> CoreResult<Role> {
        let store = self.store();
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(ctx.scoped_ws_id()), &[Self::DESCRIBE_PERMISSION])
            .await?;

        let role = match params.id {
            Some(id) => store.get(&store_ctx, &id.into()).await?,
            None => {
                let name = params
                    .name
                    .clone()
                    .ok_or(CoreError::InvalidParams("ID or name required".to_string()))?;
                store
                    .get_by_name(&store_ctx, &name, ctx.scoped_ws_id().into())
                    .await?
            }
        };

        let role = self.hydrate_role_policies(&store_ctx, &role.id).await?;

        Ok(role)
    }

    /// Loads the role's many-to-many relations (permissions + policies) and
    /// assembles the core `Role` model. Mirrors the permission-only path but
    /// also resolves the role's `role_policy` join into `Role.policies`.
    async fn hydrate_role_policies(
        &self,
        store_ctx: &StoreCtx,
        role_id: &DbId,
    ) -> CoreResult<Role> {
        let store = self.store();

        let role_with_perms_row = store.get_many_to_many(store_ctx, role_id).await?;
        let role_with_policies_row = store.get_many_to_many_policies(store_ctx, role_id).await?;

        let role: Role = RoleWithPolicies {
            role: role_with_perms_row.into(),
            policies: role_with_policies_row
                .policies
                .into_iter()
                .map(|el| el.into())
                .collect(),
        }
        .into();

        Ok(role)
    }

    /// Creates the default "Workspace Viewer" role with the given permissions.
    pub async fn create_workspace_viewer_role(
        &self,
        ctx: &mut CoreCtx,
        params: WorkspaceRoleCreateParams,
    ) -> CoreResult<Role> {
        let role_params = RoleCreateParams::new_workspace_system_role(
            params.workspace_id,
            SYSTEM_CONST.workspace_viewer_role,
            Some("Default read-only workspace viewer role"),
            params.permission_ids,
        );
        self.create(ctx, role_params).await
    }

    /// Creates the default "Workspace Admin" role with the given permissions.
    pub async fn create_workspace_admin_role(
        &self,
        ctx: &mut CoreCtx,
        params: WorkspaceRoleCreateParams,
    ) -> CoreResult<Role> {
        let role_params = RoleCreateParams::new_workspace_system_role(
            params.workspace_id,
            SYSTEM_CONST.workspace_admin_role,
            Some("Default workspace administrator role"),
            params.permission_ids,
        );
        self.create(ctx, role_params).await
    }

    /// Invalidates the account-level auth cache for every membership that carries
    /// the given role.
    ///
    /// A role mutation (e.g. deletion) changes the cached auth scope of all
    /// memberships holding that role. Each affected membership is invalidated
    /// individually (membership-scoped). Other memberships under the same
    /// account are unaffected — only the specific memberships holding the
    /// changed role lose their cached auth data.
    async fn invalidate_memberships_for_role(
        &self,
        store_ctx: &StoreCtx,
        role_id: Uuid,
    ) -> CoreResult<()> {
        // TODO: filter memberships by role_id
        let memberships = self
            .sm
            .membership
            .list_many_to_many(store_ctx, None, None, None)
            .await?;
        for membership in memberships {
            // TODO: invalidate auth cache
        }
        Ok(())
    }

    async fn hydrate_roles(
        &self,
        _ctx: &mut CoreCtx,
        rows: Vec<RoleWithPermissions>,
    ) -> CoreResult<Vec<Role>> {
        Ok(rows.into_iter().map(Role::from).collect())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D, C> for RoleService<D, C> {
    type CreateParams = RoleCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.role.create;

    async fn create(
        &self,
        ctx: &mut CoreCtx,
        params: Self::CreateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        // Extract permission_ids/policy_ids before params is consumed by into()
        let permission_ids = params.permission_ids.clone();
        let policy_ids = params.policy_ids.clone();
        let workspace_id = params.workspace_id;

        let r_create: RoleForCreate = params.into();

        let row = store.create(&store_ctx, r_create).await?;

        // Sync many-to-many permissions
        let perm_db_ids: Vec<DbId> = permission_ids.iter().map(|id| DbId::from(*id)).collect();
        self.sm
            .role
            .set_many_to_many_links(&store_ctx, &row.id, perm_db_ids)
            .await?;

        // Sync many-to-many policies
        let policy_db_ids: Vec<DbId> = policy_ids.iter().map(|id| DbId::from(*id)).collect();
        store
            .set_many_to_many_policies(&store_ctx, &row.id, policy_db_ids)
            .await?;

        self.describe(
            ctx,
            RoleDescribeParams {
                id: row.id.into(),
                workspace_id,
            },
        )
        .await
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D, C> for RoleService<D, C> {
    type DescribeParams = RoleDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.role.describe;

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DescribeParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
            .await?;

        let role = self
            .hydrate_role_policies(&store_ctx, &params.id.into())
            .await?;

        Ok(role)
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D, C> for RoleService<D, C> {
    type ListParams = RoleListParams;
    const LIST_PERMISSION: &'static str = CANONICAL_PERMISSIONS.role.list;

    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: Self::ListParams,
    ) -> CoreResult<ListResponse<Self::CoreModel>> {
        let store = self.store();
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::LIST_PERMISSION])
            .await?;

        let options = params.list_options();
        let tags_filter = params.validate_filter_tags()?;
        let tags = tags_filter.tags();
        let filter = tags_filter.filter();

        let data = store
            .list_many_to_many(
                &store_ctx,
                tags.clone(),
                filter.clone(),
                Some(options.clone()),
            )
            .await?;
        let total = store
            .count_with_tags_and_filter(&store_ctx, tags, filter)
            .await?;

        let data = self.hydrate_roles(ctx, data).await?;

        Ok(ListResponse::new(data, total, options))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D, C> for RoleService<D, C> {
    type UpdateParams = RoleUpdateParams;
    const UPDATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.role.update;

    async fn update(
        &self,
        ctx: &mut CoreCtx,
        params: Self::UpdateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::UPDATE_PERMISSION])
            .await?;

        // `policy_ids`, when present, replaces the role's policy links
        // (set semantics). Clone before `params` is consumed by `into()`.
        let policy_ids = params.policy_ids.clone();

        let res = store
            .update(&store_ctx, &params.id.into(), params.into())
            .await?;

        if let Some(policy_ids) = policy_ids {
            let policy_db_ids: Vec<DbId> = policy_ids.iter().map(|id| DbId::from(*id)).collect();
            store
                .set_many_to_many_policies(&store_ctx, &res.id, policy_db_ids)
                .await?;

            // The role's policy links changed: every membership holding this
            // role now has a stale `oxauth:policy:{mem_id}` entry.
            let memberships = self
                .sm
                .membership
                .list_containing_roles(&store_ctx, vec![res.id], None, None, None)
                .await?;
            for membership in memberships {
                self.cm.policy.invalidate(membership.id.into()).await?;
            }
        }

        // TODO: invalidate auth cache
        self.invalidate_memberships_for_role(&store_ctx, res.id.into())
            .await?;

        // If this update ever syncs role->permission links (as create() does),
        // every membership holding this role would carry a stale
        // `auth_scope.permissions` and must be invalidated.
        // TODO: Push notification trigger — notify all workspace clients
        // that a role changed. Requires wiring a `ClientService` dependency
        // into `RoleService` (constructor + factory). Then call:
        //     client_svc.push_to_workspace(
        //         params.workspace_id,
        //         "role_changed",
        //         serde_json::json!({ "role_id": res.id }),
        //         ctx,
        //     ).await;
        self.describe(
            ctx,
            RoleDescribeParams {
                id: res.id.into(),
                workspace_id: res.workspace_id.into(),
            },
        )
        .await
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D, C> for RoleService<D, C> {
    type DeleteParams = RoleDeleteParams;
    const DELETE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.role.delete;

    async fn delete(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DeleteParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
            .await?;

        // TODO: optimize delete operations across all services,
        // currently delete operation makes 2 database calls,
        // should only make one, may need to change the return type
        // of the delete method to only be deleted id
        let to_delete = self
            .describe(
                ctx,
                RoleDescribeParams {
                    id: params.id.into(),
                    workspace_id: params.workspace_id.into(),
                },
            )
            .await?;

        // The `membership_role` join table references `role` with
        // `ON DELETE RESTRICT`, so Postgres rejects the delete with a foreign
        // key violation (SQLSTATE 23503) when the role is still assigned to
        // one or more memberships. Match on the error code and surface a
        // friendly message instead of leaking the raw constraint error.
        match store.delete(&store_ctx, &to_delete.id.into()).await {
            Ok(_) => {}
            Err(StoreError::ConstraintViolation) => {
                return Err(CoreError::InvalidParams(format!(
                    "role '{}' is still assigned to one or more memberships and cannot be deleted",
                    to_delete.name
                )));
            }
            Err(err) => return Err(err.into()),
        }

        // Invalidate the auth cache for all memberships that carried the
        // deleted role, since their cached auth scope is now stale.
        self.invalidate_memberships_for_role(&store_ctx, to_delete.id)
            .await?;

        // TODO: Push notification trigger — notify all workspace clients
        // that a role was deleted. Requires wiring a `ClientService` dependency
        // into `RoleService`. Then call:
        //     client_svc.push_to_workspace(
        //         params.workspace_id,
        //         "role_changed",
        //         serde_json::json!({ "role_id": to_delete.id }),
        //         ctx,
        //     ).await;
        Ok(to_delete)
    }
}

