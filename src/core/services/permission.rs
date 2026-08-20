use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use uuid::Uuid;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            list::ListResponse,
            permission::{
                Permission, PermissionCreateManyParams, PermissionCreateParams,
                PermissionDeleteParams, PermissionDescribeParams, PermissionFilter,
                PermissionListParams, PermissionRule, PermissionUpdateParams,
            },
            role::Role,
            workspace::{Workspace, WorkspaceDescribeParams},
        },
        services::{validator::AuthValidator, workspace::WorkspaceService},
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
        crud::{Create, CreateMany, Delete, Get, GetCount, List, Update},
        ctx::StoreCtx,
        entities::{permission::PermissionRow, role::RoleFilter},
        error::StoreError,
        join::ListManyToMany,
        manager::StoreManager,
        stores::{permission::PermissionStore, role::RoleStore},
        traits::dbx::DbExecutor,
    },
};

pub struct PermissionService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    ws_svc: Arc<WorkspaceService<D, C>>,
    validator: Arc<AuthValidator>,
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D, C> for PermissionService<D, C> {
    type CoreModel = Permission;
    type ServiceStore = PermissionStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.permission
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

impl<D: DbExecutor, C: CacheExecutor> PermissionService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        ws_svc: Arc<WorkspaceService<D, C>>,
        cm: Arc<CacheManager<C>>,
        validator: Arc<AuthValidator>,
    ) -> Self {
        Self {
            sm,
            cm,
            ws_svc,
            validator,
        }
    }

    pub async fn create_many(
        &self,
        ctx: &mut CoreCtx,
        params: PermissionCreateManyParams,
    ) -> CoreResult<Vec<Permission>> {
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;
        let data = params.permissions.into_iter().map(|el| el.into()).collect();
        let res = self.store().create_many(&store_ctx, data).await?;

        let ret = res.into_iter().map(|el| el.into()).collect();

        Ok(ret)
    }

    /// Invalidates the account-level auth cache for every membership whose roles
    /// include the given permission.
    async fn invalidate_memberships_for_permission(
        &self,
        store_ctx: &StoreCtx,
        permission_id: Uuid,
    ) -> CoreResult<()> {
        let roles = self
            .sm
            .role
            .list_containing_permissions(store_ctx, vec![permission_id.into()], None, None, None)
            .await?;
        if roles.is_empty() {
            return Ok(());
        }

        let role_ids = roles.iter().map(|el| el.id).collect();
        let memberships = self
            .sm
            .membership
            .list_containing_roles(store_ctx, role_ids, None, None, None)
            .await?;

        // TODO: implement bulk invalidate method
        for membership in memberships {
            self.cm.auth.invalidate(membership.id.into()).await?;
        }
        Ok(())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D, C> for PermissionService<D, C> {
    type CreateParams = PermissionCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.permission.create;

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

        let n_perm = store.create(&store_ctx, params.into()).await?;

        Ok(n_perm.into())
    }
}
impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D, C> for PermissionService<D, C> {
    type DescribeParams = PermissionDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.permission.describe;

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

        let params = params.validate()?;

        if let Some(id) = params.id {
            let row = store.get(&store_ctx, &id.into()).await?;
            let perm = Permission::from(row);

            return Ok(perm);
        }

        Err(CoreError::InvalidParams(
            "Unable to get permission".to_string(),
        ))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D, C> for PermissionService<D, C> {
    type ListParams = PermissionListParams;
    const LIST_PERMISSION: &'static str = CANONICAL_PERMISSIONS.permission.list;

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

        // Combined query: tags (@> containment) + field filter
        let tags = tags_filter.tags();
        let filter = tags_filter.filter();

        let data = store
            .list_with_tags_and_filter(
                &store_ctx,
                tags.clone(),
                filter.clone(),
                Some(options.clone()),
            )
            .await?;
        let total = store
            .count_with_tags_and_filter(&store_ctx, tags, filter)
            .await?;

        let perms = data.into_iter().map(Permission::from).collect();

        Ok(ListResponse::new(perms, total, options))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D, C> for PermissionService<D, C> {
    type UpdateParams = PermissionUpdateParams;
    const UPDATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.permission.update;

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

        let new_name = params.name.clone().unwrap_or_default();

        let updated = match store
            .update(&store_ctx, &params.id.into(), params.into())
            .await
        {
            Ok(updated) => updated,
            Err(StoreError::ConstraintViolation) => {
                return Err(CoreError::AlreadyExists(format!(
                    "a permission named '{new_name}' already exists in this workspace"
                )));
            }
            Err(err) => return Err(err.into()),
        };

        // Invalidate the auth cache for all memberships whose roles grant the
        // updated permission — its name may have changed.
        self.invalidate_memberships_for_permission(&store_ctx, updated.id.into())
            .await?;

        // TODO: Push notification trigger — notify all workspace clients
        // that a permission changed. Requires wiring a `ClientService`
        // dependency into `PermissionService` (constructor + factory). Then
        // call:
        //     client_svc.push_to_workspace(
        //         params.workspace_id,
        //         "permission_changed",
        //         serde_json::json!({ "permission_id": updated.id }),
        //         ctx,
        //     ).await;
        self.describe(
            ctx,
            PermissionDescribeParams {
                id: Some(updated.id.into()),
                workspace_id: Some(updated.workspace_id),
            },
        )
        .await
    }
}
impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D, C> for PermissionService<D, C> {
    type DeleteParams = PermissionDeleteParams;
    const DELETE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.permission.delete;

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

        let to_delete = self
            .describe(
                ctx,
                PermissionDescribeParams {
                    id: Some(params.id.into()),
                    workspace_id: params.workspace_id,
                },
            )
            .await?;

        let res = match store.delete(&store_ctx, &to_delete.id.into()).await {
            Ok(res) => res,
            Err(StoreError::ConstraintViolation) => {
                return Err(CoreError::InvalidParams(format!(
                    "permission '{}' is still attached to one or more roles and cannot be deleted",
                    to_delete.key
                )));
            }
            Err(err) => return Err(err.into()),
        };

        // Invalidate the auth cache for all memberships whose roles granted the
        // deleted permission — their cached auth scopes are now stale.
        self.invalidate_memberships_for_permission(&store_ctx, res.id.into())
            .await?;

        // TODO: Push notification trigger — notify all workspace clients
        // that a permission was deleted. Requires wiring a `ClientService`
        // dependency into `PermissionService`. Then call:
        //     client_svc.push_to_workspace(
        //         params.workspace_id,
        //         "permission_changed",
        //         serde_json::json!({ "permission_id": to_delete.id }),
        //         ctx,
        //     ).await;
        Ok(to_delete)
    }
}

// ============================================================================
// Per-domain permission structs
// ============================================================================

pub struct AccountPermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl AccountPermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Create new accounts"),
                (self.describe, "View account details"),
                (self.list, "List accounts"),
                (self.update, "Update account details"),
                (self.delete, "Delete accounts"),
            ]
        })
    }
}

pub struct WorkspacePermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl WorkspacePermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Create new workspaces"),
                (self.describe, "View workspace details"),
                (self.list, "List workspaces"),
                (self.update, "Update workspace settings"),
                (self.delete, "Delete workspaces"),
            ]
        })
    }
}

pub struct ProjectPermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl ProjectPermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Create new projects"),
                (self.describe, "View project details"),
                (self.list, "List projects"),
                (self.update, "Update project settings"),
                (self.delete, "Delete projects"),
            ]
        })
    }
}

pub struct ProfilePermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl ProfilePermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Create new profiles"),
                (self.describe, "View profile details"),
                (self.list, "List profiles"),
                (self.update, "Update profile settings"),
                (self.delete, "Delete profiles"),
            ]
        })
    }
}

pub struct MembershipPermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl MembershipPermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Invite members to workspace"),
                (self.describe, "View membership details"),
                (self.list, "List memberships"),
                (self.update, "Update membership roles"),
                (self.delete, "Remove members"),
            ]
        })
    }
}

pub struct RolePermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl RolePermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Create new roles"),
                (self.describe, "View role details"),
                (self.list, "List roles"),
                (self.update, "Update role permissions"),
                (self.delete, "Delete roles"),
            ]
        })
    }
}

pub struct ClientPermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
    pub validate: &'static str,
    pub regenerate_secret: &'static str,
}

impl ClientPermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 7]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Register new API clients"),
                (self.describe, "View client details"),
                (self.list, "List clients"),
                (self.update, "Update client configuration"),
                (self.delete, "Delete clients"),
                (self.validate, "Validate client credentials"),
                (self.regenerate_secret, "Regenerate client secret"),
            ]
        })
    }
}

pub struct CredentialPermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl CredentialPermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Create new credentials"),
                (self.describe, "View credential details"),
                (self.list, "List credentials"),
                (self.update, "Update credentials"),
                (self.delete, "Delete credentials"),
            ]
        })
    }
}

pub struct PermissionPermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl PermissionPermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Create new permissions"),
                (self.describe, "View permission details"),
                (self.list, "List permissions"),
                (self.update, "Update permissions"),
                (self.delete, "Delete permissions"),
            ]
        })
    }
}

pub struct PolicyPermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl PolicyPermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Create new policies"),
                (self.describe, "View policy details"),
                (self.list, "List policies"),
                (self.update, "Update policies"),
                (self.delete, "Delete policies"),
            ]
        })
    }
}

pub struct AuthPermissions {
    pub refresh: &'static str,
    pub revoke: &'static str,
}

impl AuthPermissions {
    pub fn all(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            (
                self.refresh,
                "Rotate authentication tokens (refresh access)",
            ),
            (
                self.revoke,
                "Revoke authentication tokens (invalidate sessions)",
            ),
        ]
    }
}

pub struct SystemPermissions {
    pub star_all: &'static str,
    pub sys_admin: &'static str,
    pub sys_prefix: &'static str,
    pub workspace_prefix: &'static str,
}

impl SystemPermissions {
    pub fn all(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            (self.sys_admin, "Complete system access"),
            (self.sys_prefix, "System prefix (resricted)"),
            (self.workspace_prefix, "Workspace prefix (restricted)"),
            (self.star_all, "Star all permissions *"),
        ]
    }
}

// ============================================================================
// Canonical permissions aggregate
// ============================================================================

pub struct CanonicalPermissions {
    pub account: AccountPermissions,
    pub workspace: WorkspacePermissions,
    pub project: ProjectPermissions,
    pub profile: ProfilePermissions,
    pub membership: MembershipPermissions,
    pub role: RolePermissions,
    pub client: ClientPermissions,
    pub credential: CredentialPermissions,
    pub permission: PermissionPermissions,
    pub policy: PolicyPermissions,
    pub auth: AuthPermissions,
    pub system: SystemPermissions,
}

impl CanonicalPermissions {
    /// Returns all canonical permissions as (name, description) tuples across all domains.
    pub fn all(&self) -> Vec<(&'static str, &'static str)> {
        let mut v = Vec::new();
        v.extend_from_slice(self.account.all());
        v.extend_from_slice(self.workspace.all());
        v.extend_from_slice(self.project.all());
        v.extend_from_slice(self.profile.all());
        v.extend_from_slice(self.membership.all());
        v.extend_from_slice(self.role.all());
        v.extend_from_slice(self.client.all());
        v.extend_from_slice(self.credential.all());
        v.extend_from_slice(self.permission.all());
        v.extend_from_slice(self.policy.all());
        v.extend_from_slice(&self.auth.all());
        v.extend_from_slice(&self.system.all());
        v
    }

    pub fn default_workspace_admin_perms(&self) -> Vec<(&'static str, &'static str)> {
        let mut v = Vec::new();
        v.extend_from_slice(self.account.all());
        // only expose workspace describe, in order for members to decribe their own workspace
        v.extend_from_slice(&[(self.workspace.describe, "Describe the workspace")]);
        v.extend_from_slice(self.project.all());
        v.extend_from_slice(self.profile.all());
        v.extend_from_slice(self.membership.all());
        v.extend_from_slice(self.role.all());
        v.extend_from_slice(self.client.all());
        v.extend_from_slice(self.credential.all());
        v.extend_from_slice(self.permission.all());
        v.extend_from_slice(self.policy.all());
        v.extend_from_slice(&self.auth.all());
        v
    }

    pub fn default_workspace_viewer_perms(&self) -> Vec<&'static str> {
        let v = vec![
            // account
            CANONICAL_PERMISSIONS.account.describe, // describe own account
            CANONICAL_PERMISSIONS.account.update,   // update own account
            // workspace
            CANONICAL_PERMISSIONS.workspace.describe, // describe own workspace
            // project
            CANONICAL_PERMISSIONS.project.describe, // describe projects
            CANONICAL_PERMISSIONS.project.list,     // list projects
            // profile
            CANONICAL_PERMISSIONS.profile.describe, // describe profiles
            CANONICAL_PERMISSIONS.profile.list,     // list profiles
            // membership
            CANONICAL_PERMISSIONS.membership.describe, // describe memberships
            CANONICAL_PERMISSIONS.membership.list,     // list memberships
            // auth
            CANONICAL_PERMISSIONS.auth.refresh,
            CANONICAL_PERMISSIONS.auth.revoke,
        ];

        v
    }
}

pub const CANONICAL_PERMISSIONS: CanonicalPermissions = CanonicalPermissions {
    account: AccountPermissions {
        create: "account:create",
        describe: "account:describe",
        list: "account:list",
        update: "account:update",
        delete: "account:delete",
    },
    workspace: WorkspacePermissions {
        create: "workspace:create",
        describe: "workspace:describe",
        list: "workspace:list",
        update: "workspace:update",
        delete: "workspace:delete",
    },
    project: ProjectPermissions {
        create: "project:create",
        describe: "project:describe",
        list: "project:list",
        update: "project:update",
        delete: "project:delete",
    },
    profile: ProfilePermissions {
        create: "profile:create",
        describe: "profile:describe",
        list: "profile:list",
        update: "profile:update",
        delete: "profile:delete",
    },
    membership: MembershipPermissions {
        create: "membership:create",
        describe: "membership:describe",
        list: "membership:list",
        update: "membership:update",
        delete: "membership:delete",
    },
    role: RolePermissions {
        create: "role:create",
        describe: "role:describe",
        list: "role:list",
        update: "role:update",
        delete: "role:delete",
    },
    client: ClientPermissions {
        create: "client:create",
        describe: "client:describe",
        list: "client:list",
        update: "client:update",
        delete: "client:delete",
        validate: "client:validate",
        regenerate_secret: "client:regenerateSecret",
    },
    credential: CredentialPermissions {
        create: "credential:create",
        describe: "credential:describe",
        list: "credential:list",
        update: "credential:update",
        delete: "credential:delete",
    },
    permission: PermissionPermissions {
        create: "permission:create",
        describe: "permission:describe",
        list: "permission:list",
        update: "permission:update",
        delete: "permission:delete",
    },
    policy: PolicyPermissions {
        create: "policy:create",
        describe: "policy:describe",
        list: "policy:list",
        update: "policy:update",
        delete: "policy:delete",
    },
    auth: AuthPermissions {
        refresh: "auth:refresh",
        revoke: "auth:revoke",
    },
    system: SystemPermissions {
        sys_admin: "*:*",
        sys_prefix: "system",
        workspace_prefix: "workspace",
        star_all: "*",
    },
};
