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
                Permission, PermissionCreateParams, PermissionDeleteParams,
                PermissionDescribeParams, PermissionListParams, PermissionRule,
                PermissionUpdateParams,
            },
            role::Role,
            workspace::{Workspace, WorkspaceDescribeParams},
        },
        services::{auth::AuthValidator, workspace::WorkspaceService},
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
        crud::{Create, Delete, Get, GetCount, List, Update},
        ctx::StoreCtx,
        entities::permission::PermissionRow,
        join::ListManyToMany,
        manager::StoreManager,
        stores::{permission::PermissionStore, role::RoleStore},
        traits::dbx::DbExecutor,
    },
};

pub struct PermissionService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    ws_svc: WorkspaceService<D, C>,
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D, C> for PermissionService<D, C> {
    type CoreModel = Permission;
    type ServiceStore = PermissionStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.permission
    }

    fn ws_svc(&self) -> &WorkspaceService<D, C> {
        &self.ws_svc
    }
}

impl<D: DbExecutor, C: CacheExecutor> PermissionService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        ws_svc: WorkspaceService<D, C>,
        cm: Arc<CacheManager<C>>,
    ) -> Self {
        Self { sm, cm, ws_svc }
    }

    /// Invalidates the account-level auth cache for every membership whose roles
    /// include the given permission.
    ///
    /// Changing a permission (its name or its deletion) affects the cached
    /// auth scope of every membership holding a role that grants it. Collecting
    /// distinct account IDs and invalidating per-account avoids redundant
    /// per-membership cache purge calls.
    async fn invalidate_memberships_for_permission(
        &self,
        store_ctx: &StoreCtx,
        permission_id: Uuid,
    ) -> CoreResult<()> {
        // Find the roles that include this permission.
        let roles = self
            .sm
            .role
            .list_many_to_many(store_ctx, None, None, None)
            .await?;
        let affected_role_ids: Vec<Uuid> = roles
            .iter()
            .filter(|r| {
                r.permissions
                    .iter()
                    .any(|p| Uuid::from(p.id) == permission_id)
            })
            .map(|r| Uuid::from(r.role.id))
            .collect();
        if affected_role_ids.is_empty() {
            return Ok(());
        }

        // Invalidate the auth cache for each affected membership individually.
        // This is membership-scoped: only memberships holding the changed
        // permission are invalidated. Other memberships under the same account
        // are unaffected, preserving acc_version across unrelated memberships.
        let memberships = self
            .sm
            .membership
            .list_many_to_many(store_ctx, None, None, None)
            .await?;
        for membership in memberships {
            if membership
                .roles
                .iter()
                .any(|r| affected_role_ids.contains(&Uuid::from(r.id)))
            {
                self.cm
                    .invalidation
                    .invalidate(membership.id.into(), membership.membership.account_id, None)
                    .await?;
            }
        }
        Ok(())
    }

    async fn hydrate_permissions(
        &self,
        ctx: &mut CoreCtx,
        rows: Vec<PermissionRow>,
    ) -> CoreResult<Vec<Permission>> {
        let mut workspaces: HashMap<Uuid, Workspace> = HashMap::new();

        let mut perms: Vec<Permission> = Vec::with_capacity(rows.len());

        // // Hydrate results
        for row in rows.into_iter() {
            let workspace_id: Uuid = row.workspace_id;
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
            let perm = Permission::from_row_with_entities(row, workspace.clone())?;
            perms.push(perm);
        }

        Ok(perms)
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
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        // TODO: ensure cannot create same permission in same workspace
        // check database constraints

        let n_perm = store.create(&store_ctx, params.into()).await?;

        self.describe(
            ctx,
            PermissionDescribeParams {
                id: Some(n_perm.id.into()),
                workspace_id: n_perm.workspace_id.into(),
            },
        )
        .await
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
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
            .await?;

        let params = params.validate()?;
        let ws = self
            .ws_svc
            .describe(
                ctx,
                WorkspaceDescribeParams {
                    id: Some(params.workspace_id),
                    slug: None,
                },
            )
            .await?;

        if let Some(id) = params.id {
            let row = store.get(&store_ctx, &id.into()).await?;
            let perm = Permission::from_row_with_entities(row, ws)?;

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
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::LIST_PERMISSION])
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

        let perms = self.hydrate_permissions(ctx, data).await?;

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
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::UPDATE_PERMISSION])
            .await?;

        // TODO: ensure cannot update permission to an existing permission in same workspace
        // check database constraints
        let updated = store
            .update(&store_ctx, &params.id.into(), params.into())
            .await?;

        // Invalidate the auth cache for all memberships whose roles grant the
        // updated permission — its name may have changed.
        self.invalidate_memberships_for_permission(&store_ctx, updated.id.into())
            .await?;

        // TODO(T031): Push notification trigger — notify all workspace clients
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
                workspace_id: updated.workspace_id,
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
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
            .await?;

        // TODO: ensure cannot delete attached permission
        // check database constraints
        let to_delete = self
            .describe(
                ctx,
                PermissionDescribeParams {
                    id: Some(params.id.into()),
                    workspace_id: params.workspace_id,
                },
            )
            .await?;

        let res = store.delete(&store_ctx, &to_delete.id.into()).await?;

        // Invalidate the auth cache for all memberships whose roles granted the
        // deleted permission — their cached auth scopes are now stale.
        self.invalidate_memberships_for_permission(&store_ctx, res.id.into())
            .await?;

        // TODO(T031): Push notification trigger — notify all workspace clients
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

// ============================================================================
// Canonical permissions aggregate
// ============================================================================

pub struct CanonicalPermissions {
    pub account: AccountPermissions,
    pub workspace: WorkspacePermissions,
    pub project: ProjectPermissions,
    pub membership: MembershipPermissions,
    pub role: RolePermissions,
    pub client: ClientPermissions,
    pub credential: CredentialPermissions,
    pub permission: PermissionPermissions,
}

impl CanonicalPermissions {
    /// Returns all canonical permissions as (name, description) tuples across all domains.
    pub fn all(&self) -> Vec<(&'static str, &'static str)> {
        let mut v = Vec::new();
        v.extend_from_slice(self.account.all());
        v.extend_from_slice(self.workspace.all());
        v.extend_from_slice(self.project.all());
        v.extend_from_slice(self.membership.all());
        v.extend_from_slice(self.role.all());
        v.extend_from_slice(self.client.all());
        v.extend_from_slice(self.credential.all());
        v.extend_from_slice(self.permission.all());
        v
    }

    pub fn default_workspace_viewer_perms(&self) -> Vec<&'static str> {
        let v = vec![
            // account
            CANONICAL_PERMISSIONS.account.describe, // describe own account
            CANONICAL_PERMISSIONS.account.update,   // update own account
            // workspace
            CANONICAL_PERMISSIONS.workspace.describe, // describe own workspace
        ];

        v
    }

    pub fn default_workspace_admin_perms(&self) -> Vec<&'static str> {
        let mut all = Vec::new();
        all.extend_from_slice(self.account.all());
        // all.extend_from_slice(self.workspace.all());
        all.extend_from_slice(self.project.all());
        all.extend_from_slice(self.membership.all());
        all.extend_from_slice(self.role.all());
        all.extend_from_slice(self.client.all());
        all.extend_from_slice(self.credential.all());
        all.extend_from_slice(self.permission.all());
        all.into_iter().map(|(name, _)| name).collect()
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
};
