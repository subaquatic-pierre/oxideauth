use std::{collections::HashMap, sync::Arc};

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
        entities::{
            permission::{PermissionForCreate, PermissionRow},
            role::RoleFilter,
        },
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

        let mut data: Vec<PermissionForCreate> = vec![];
        for param in params.permissions.into_iter() {
            Self::validate_restricted_key(&param.key)?;
            data.push(param.into());
        }

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

    fn validate_restricted_key(key: &str) -> CoreResult<()> {
        if CANONICAL_PERMISSIONS
            .system
            .all()
            .iter()
            .any(|el| key.starts_with(el.key))
        {
            return Err(CoreError::InvalidParams(format!(
                "permission key is reserved {key}"
            )));
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

        // ensure no keys are system restricted keys
        Self::validate_restricted_key(&params.key)?;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalPermission {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

const ACCOUNT: [CanonicalPermission; 5] = [
    CanonicalPermission { key: "account:create", label: "Create Account", description: "Create new accounts" },
    CanonicalPermission { key: "account:describe", label: "Describe Account", description: "View account details" },
    CanonicalPermission { key: "account:list", label: "List Accounts", description: "List accounts" },
    CanonicalPermission { key: "account:update", label: "Update Account", description: "Update account details" },
    CanonicalPermission { key: "account:delete", label: "Delete Account", description: "Delete accounts" },
];
const WORKSPACE: [CanonicalPermission; 5] = [
    CanonicalPermission { key: "workspace:create", label: "Create Workspace", description: "Create new workspaces" },
    CanonicalPermission { key: "workspace:describe", label: "Describe Workspace", description: "View workspace details" },
    CanonicalPermission { key: "workspace:list", label: "List Workspaces", description: "List workspaces" },
    CanonicalPermission { key: "workspace:update", label: "Update Workspace", description: "Update workspace settings" },
    CanonicalPermission { key: "workspace:delete", label: "Delete Workspace", description: "Delete workspaces" },
];
const PROJECT: [CanonicalPermission; 5] = [
    CanonicalPermission { key: "project:create", label: "Create Project", description: "Create new projects" },
    CanonicalPermission { key: "project:describe", label: "Describe Project", description: "View project details" },
    CanonicalPermission { key: "project:list", label: "List Projects", description: "List projects" },
    CanonicalPermission { key: "project:update", label: "Update Project", description: "Update project settings" },
    CanonicalPermission { key: "project:delete", label: "Delete Project", description: "Delete projects" },
];
const PROFILE: [CanonicalPermission; 5] = [
    CanonicalPermission { key: "profile:create", label: "Create Profile", description: "Create new profiles" },
    CanonicalPermission { key: "profile:describe", label: "Describe Profile", description: "View profile details" },
    CanonicalPermission { key: "profile:list", label: "List Profiles", description: "List profiles" },
    CanonicalPermission { key: "profile:update", label: "Update Profile", description: "Update profile settings" },
    CanonicalPermission { key: "profile:delete", label: "Delete Profile", description: "Delete profiles" },
];
const MEMBERSHIP: [CanonicalPermission; 5] = [
    CanonicalPermission { key: "membership:create", label: "Create Membership", description: "Invite members to workspace" },
    CanonicalPermission { key: "membership:describe", label: "Describe Membership", description: "View membership details" },
    CanonicalPermission { key: "membership:list", label: "List Memberships", description: "List memberships" },
    CanonicalPermission { key: "membership:update", label: "Update Membership", description: "Update membership roles" },
    CanonicalPermission { key: "membership:delete", label: "Delete Membership", description: "Remove members" },
];
const ROLE: [CanonicalPermission; 5] = [
    CanonicalPermission { key: "role:create", label: "Create Role", description: "Create new roles" },
    CanonicalPermission { key: "role:describe", label: "Describe Role", description: "View role details" },
    CanonicalPermission { key: "role:list", label: "List Roles", description: "List roles" },
    CanonicalPermission { key: "role:update", label: "Update Role", description: "Update role permissions" },
    CanonicalPermission { key: "role:delete", label: "Delete Role", description: "Delete roles" },
];
const CLIENT: [CanonicalPermission; 7] = [
    CanonicalPermission { key: "client:create", label: "Create Client", description: "Register new API clients" },
    CanonicalPermission { key: "client:describe", label: "Describe Client", description: "View client details" },
    CanonicalPermission { key: "client:list", label: "List Clients", description: "List clients" },
    CanonicalPermission { key: "client:update", label: "Update Client", description: "Update client configuration" },
    CanonicalPermission { key: "client:delete", label: "Delete Client", description: "Delete clients" },
    CanonicalPermission { key: "client:validate", label: "Validate Client", description: "Validate client credentials" },
    CanonicalPermission { key: "client:regenerateSecret", label: "Regenerate Client Secret", description: "Regenerate client secret" },
];
const CREDENTIAL: [CanonicalPermission; 5] = [
    CanonicalPermission { key: "credential:create", label: "Create Credential", description: "Create new credentials" },
    CanonicalPermission { key: "credential:describe", label: "Describe Credential", description: "View credential details" },
    CanonicalPermission { key: "credential:list", label: "List Credentials", description: "List credentials" },
    CanonicalPermission { key: "credential:update", label: "Update Credential", description: "Update credentials" },
    CanonicalPermission { key: "credential:delete", label: "Delete Credential", description: "Delete credentials" },
];
const PERMISSION: [CanonicalPermission; 5] = [
    CanonicalPermission { key: "permission:create", label: "Create Permission", description: "Create new permissions" },
    CanonicalPermission { key: "permission:describe", label: "Describe Permission", description: "View permission details" },
    CanonicalPermission { key: "permission:list", label: "List Permissions", description: "List permissions" },
    CanonicalPermission { key: "permission:update", label: "Update Permission", description: "Update permissions" },
    CanonicalPermission { key: "permission:delete", label: "Delete Permission", description: "Delete permissions" },
];
const POLICY: [CanonicalPermission; 5] = [
    CanonicalPermission { key: "policy:create", label: "Create Policy", description: "Create new policies" },
    CanonicalPermission { key: "policy:describe", label: "Describe Policy", description: "View policy details" },
    CanonicalPermission { key: "policy:list", label: "List Policies", description: "List policies" },
    CanonicalPermission { key: "policy:update", label: "Update Policy", description: "Update policies" },
    CanonicalPermission { key: "policy:delete", label: "Delete Policy", description: "Delete policies" },
];
const AUTH: [CanonicalPermission; 2] = [
    CanonicalPermission { key: "auth:refresh", label: "Refresh Tokens", description: "Rotate authentication tokens (refresh access)" },
    CanonicalPermission { key: "auth:revoke", label: "Revoke Tokens", description: "Revoke authentication tokens (invalidate sessions)" },
];
const SYSTEM: [CanonicalPermission; 4] = [
    CanonicalPermission { key: "*:*", label: "System Administrator", description: "Complete system access" },
    CanonicalPermission { key: "system", label: "System Prefix", description: "System prefix (resricted)" },
    CanonicalPermission { key: "workspace", label: "Workspace Prefix", description: "Workspace prefix (restricted)" },
    CanonicalPermission { key: "*", label: "All Permissions", description: "Star all permissions *" },
];

pub struct AccountPermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl AccountPermissions {
    pub fn all(&self) -> &[CanonicalPermission] {
        &ACCOUNT
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
    pub fn all(&self) -> &[CanonicalPermission] {
        &WORKSPACE
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
    pub fn all(&self) -> &[CanonicalPermission] {
        &PROJECT
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
    pub fn all(&self) -> &[CanonicalPermission] {
        &PROFILE
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
    pub fn all(&self) -> &[CanonicalPermission] {
        &MEMBERSHIP
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
    pub fn all(&self) -> &[CanonicalPermission] {
        &ROLE
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
    pub fn all(&self) -> &[CanonicalPermission] {
        &CLIENT
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
    pub fn all(&self) -> &[CanonicalPermission] {
        &CREDENTIAL
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
    pub fn all(&self) -> &[CanonicalPermission] {
        &PERMISSION
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
    pub fn all(&self) -> &[CanonicalPermission] {
        &POLICY
    }
}

pub struct AuthPermissions {
    pub refresh: &'static str,
    pub revoke: &'static str,
}

impl AuthPermissions {
    pub fn all(&self) -> &[CanonicalPermission] {
        &AUTH
    }
}

pub struct SystemPermissions {
    pub star_all: &'static str,
    pub sys_admin: &'static str,
    pub sys_prefix: &'static str,
    pub workspace_prefix: &'static str,
}

impl SystemPermissions {
    pub fn all(&self) -> &[CanonicalPermission] {
        &SYSTEM
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
    pub fn all(&self) -> &[CanonicalPermission] {
        static ALL: [CanonicalPermission; 58] = [
            ACCOUNT[0], ACCOUNT[1], ACCOUNT[2], ACCOUNT[3], ACCOUNT[4],
            WORKSPACE[0], WORKSPACE[1], WORKSPACE[2], WORKSPACE[3], WORKSPACE[4],
            PROJECT[0], PROJECT[1], PROJECT[2], PROJECT[3], PROJECT[4],
            PROFILE[0], PROFILE[1], PROFILE[2], PROFILE[3], PROFILE[4],
            MEMBERSHIP[0], MEMBERSHIP[1], MEMBERSHIP[2], MEMBERSHIP[3], MEMBERSHIP[4],
            ROLE[0], ROLE[1], ROLE[2], ROLE[3], ROLE[4],
            CLIENT[0], CLIENT[1], CLIENT[2], CLIENT[3], CLIENT[4], CLIENT[5], CLIENT[6],
            CREDENTIAL[0], CREDENTIAL[1], CREDENTIAL[2], CREDENTIAL[3], CREDENTIAL[4],
            PERMISSION[0], PERMISSION[1], PERMISSION[2], PERMISSION[3], PERMISSION[4],
            POLICY[0], POLICY[1], POLICY[2], POLICY[3], POLICY[4],
            AUTH[0], AUTH[1], SYSTEM[0], SYSTEM[1], SYSTEM[2], SYSTEM[3],
        ];
        &ALL
    }

    pub fn default_workspace_admin_perms(&self) -> &[CanonicalPermission] {
        static ADMIN: [CanonicalPermission; 50] = [
            ACCOUNT[0], ACCOUNT[1], ACCOUNT[2], ACCOUNT[3], ACCOUNT[4], WORKSPACE[1],
            PROJECT[0], PROJECT[1], PROJECT[2], PROJECT[3], PROJECT[4],
            PROFILE[0], PROFILE[1], PROFILE[2], PROFILE[3], PROFILE[4],
            MEMBERSHIP[0], MEMBERSHIP[1], MEMBERSHIP[2], MEMBERSHIP[3], MEMBERSHIP[4],
            ROLE[0], ROLE[1], ROLE[2], ROLE[3], ROLE[4],
            CLIENT[0], CLIENT[1], CLIENT[2], CLIENT[3], CLIENT[4], CLIENT[5], CLIENT[6],
            CREDENTIAL[0], CREDENTIAL[1], CREDENTIAL[2], CREDENTIAL[3], CREDENTIAL[4],
            PERMISSION[0], PERMISSION[1], PERMISSION[2], PERMISSION[3], PERMISSION[4],
            POLICY[0], POLICY[1], POLICY[2], POLICY[3], POLICY[4], AUTH[0], AUTH[1],
        ];
        &ADMIN
    }

    pub fn default_workspace_viewer_perms(&self) -> &[CanonicalPermission] {
        static VIEWER: [CanonicalPermission; 11] = [
            ACCOUNT[1], ACCOUNT[3], WORKSPACE[1], PROJECT[1], PROJECT[2],
            PROFILE[1], PROFILE[2], MEMBERSHIP[1], MEMBERSHIP[2], AUTH[0], AUTH[1],
        ];
        &VIEWER
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
