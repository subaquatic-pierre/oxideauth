use uuid::Uuid;

use crate::cache::entities::auth::{AuthCache, AuthScopeCache};
use crate::cache::entities::workspace::WorkspaceCache;
use crate::core::models::workspace::{WorkspaceConfig, WorkspaceMeta};
use crate::store::stores::workspace::SYSTEM_CONST;
use crate::{
    core::{
        error::{CoreError, CoreResult},
        models::{
            permission::{PermissionEngine, PermissionRule},
            workspace::Workspace,
        },
    },
    store::{ctx::StoreCtx, manager::StoreManager, traits::dbx::DbExecutor},
};
use std::sync::OnceLock;

/// The request-scoped operational context, resolved by the auth middleware.
///
/// `auth_cache` is the single source of truth for the authenticated identity:
/// account ID, membership ID, token-scoped workspace, and permissions.
///
/// `scoped_ws_id` is the **operational target** — the workspace this request
/// acts on. For scoped tokens it equals the token's workspace. For system/root
/// tokens the middleware sets it from the `X-Workspace-Id` header.
#[derive(Clone, Debug)]
pub struct CoreCtx {
    pub auth_cache: AuthCache,
    pub ws_cache: WorkspaceCache,
    perm_checker: PermissionEngine,
}

impl CoreCtx {
    pub fn new(auth_cache: AuthCache, ws_cache: WorkspaceCache) -> CoreResult<Self> {
        let perm_checker =
            PermissionEngine::from_string_vec(auth_cache.auth_scope.permissions.clone())?;
        Ok(Self {
            auth_cache,
            ws_cache,
            perm_checker,
        })
    }

    /// Creates a bootstrap context with nil UUIDs — no pre-existing DB data required.
    ///
    /// Used during seeding/initialization before any accounts or workspaces exist.
    /// Has `*:*` permissions and nil scoped workspace (no row-level filtering).
    pub fn bootstrap() -> CoreResult<Self> {
        let auth_cache = AuthCache::bootstrap();
        let ws_cache = WorkspaceCache::bootstrap();
        // let ws_cache = WorkspaceCache::bootstrap();
        let perm_checker = PermissionEngine::from_string_vec(vec!["*:*".to_string()])?;
        Ok(Self {
            auth_cache,
            ws_cache,
            perm_checker,
        })
    }

    pub fn permission_checker(&self) -> &PermissionEngine {
        &self.perm_checker
    }

    pub fn extend_perms(&mut self, perms: &[&str]) -> CoreResult<()> {
        let new_perms: Vec<PermissionRule> = PermissionRule::perms_from_str_slice(perms)?;
        self.perm_checker.extend(new_perms);
        Ok(())
    }

    // --- Identity (from auth_cache — the token's claims) ---

    pub fn account_id(&self) -> Uuid {
        self.auth_cache.acc_id
    }

    pub fn membership_id(&self) -> Uuid {
        self.auth_cache.mem_id
    }

    /// Whether the token is scoped to the system/root workspace (admin).
    pub fn is_system_workspace(&self) -> CoreResult<bool> {
        // Note: slug is unique across the system
        // only one system_ws_slug can ever exist
        // it is created on system bootstrap
        Ok(&self.ws_cache.slug == SYSTEM_CONST.system_ws_slug)
    }

    // --- Operational target (may differ from token scope for root tokens) ---

    /// The workspace this request actually operates on.
    ///
    /// Set during context resolution to the token's workspace for scoped users,
    /// or to the `X-Workspace-Id` header value for system/root tokens.
    pub fn scoped_ws_id(&self) -> Uuid {
        self.ws_cache.id
    }

    /// Override the operational workspace target (used by middleware for
    /// system tokens that supply an `X-Workspace-Id` header).
    pub fn set_scoped_ws(&mut self, ws_cache: WorkspaceCache) {
        self.ws_cache = ws_cache;
    }
}

impl From<CoreCtx> for StoreCtx {
    fn from(ctx: CoreCtx) -> Self {
        Self::new(ctx.auth_cache.acc_id, ctx.scoped_ws_id())
    }
}

impl From<&CoreCtx> for StoreCtx {
    fn from(ctx: &CoreCtx) -> Self {
        Self::new(ctx.auth_cache.acc_id, ctx.scoped_ws_id())
    }
}

impl From<&mut CoreCtx> for StoreCtx {
    fn from(ctx: &mut CoreCtx) -> Self {
        Self::new(ctx.auth_cache.acc_id, ctx.scoped_ws_id())
    }
}

/// Resolves and caches the UUIDs of well-known entities (system workspace, root
/// account) so that context factories like `bootstrap()` and `system()` use real
/// DB-backed identities instead of hardcoded strings.
///
/// # Lifecycle
///
/// 1. Created empty at startup.
/// 2. Initialized via `init_from_seed()` after `seed_all` creates the system
///    workspace and root account.
/// 3. From that point on, `bootstrap()` and `system()` return contexts with real
///    UUIDs.
pub struct ContextFactory {
    system_ws_id: OnceLock<Uuid>,
    system_user_id: OnceLock<Uuid>,
}

impl ContextFactory {
    /// Creates an empty factory. Call `init_from_seed()` before using
    /// `bootstrap()` or `system()` in production.
    pub fn new() -> Self {
        Self {
            system_ws_id: OnceLock::new(),
            system_user_id: OnceLock::new(),
        }
    }

    /// Looks up the system workspace (by slug `"system"`) and the root account
    /// (by email `"root@system.local"`) in the database and caches their UUIDs.
    ///
    /// Must be called after `seed_all` has completed.
    pub async fn init_from_seed<D: DbExecutor>(&self, sm: &StoreManager<D>) -> CoreResult<()> {
        let store_ctx = StoreCtx::bootstrap();

        // Look up system workspace by slug
        let system_ws = sm.workspace.get_system_ws(&store_ctx).await?;

        self.system_ws_id.set(system_ws.id.into()).map_err(|_| {
            CoreError::InvalidParams("ContextFactory already initialized".to_string())
        })?;

        // Look up root account by email
        let system_acc = sm.account.get_system_acc(&store_ctx).await?;
        self.system_user_id.set(system_acc.id.into()).map_err(|_| {
            CoreError::InvalidParams("ContextFactory already initialized".to_string())
        })?;

        Ok(())
    }

    /// Returns the cached system workspace UUID.
    pub fn system_ws_id(&self) -> Uuid {
        *self
            .system_ws_id
            .get()
            .expect("System workspace not initialized")
    }

    /// Returns the cached root account UUID.
    pub fn system_user_id(&self) -> Uuid {
        *self
            .system_user_id
            .get()
            .expect("System account not initialized")
    }

    /// Creates a system-level `CoreCtx` authenticated as the system account.
    ///
    /// Uses the cached system workspace and account UUIDs so all audit fields
    /// (`created_by`, `updated_by`) carry a real, traceable identity. Has `*:*`
    /// permissions for unrestricted system operations.
    ///
    /// Must only be used after `init_from_seed()` has populated the cached UUIDs.
    pub fn system(&self) -> CoreResult<CoreCtx> {
        let ws_id = self.system_ws_id.get().ok_or(CoreError::Auth(
            "System workspace not initialized".to_string(),
        ))?;
        let acc_id = self.system_user_id.get().ok_or(CoreError::Auth(
            "System account not initialized".to_string(),
        ))?;

        let auth_cache = AuthCache {
            mem_id: Uuid::nil(), // system account has no membership
            acc_id: *acc_id,
            sid: None,
            mem_version: 0,
            acc_version: 0,
            mem_active: true,
            acc_enabled: true,
            auth_scope: AuthScopeCache::system(),
        };

        let ws_cache = WorkspaceCache {
            id: ws_id.clone(),
            name: String::new(),
            slug: String::new(),
            description: None,
            owner: Uuid::nil(),
            config: WorkspaceConfig::default(),
            tags: vec![],
            meta: WorkspaceMeta::default(),
        };

        let perm_checker = PermissionEngine::from_string_vec(vec!["*:*".to_string()])?;
        Ok(CoreCtx {
            auth_cache,
            ws_cache,
            perm_checker,
        })
    }
}

impl Default for ContextFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use crate::dev::init::init_test;

    use super::*;

    fn setup_checker() -> CoreResult<PermissionEngine> {
        PermissionEngine::from_str_slice(&["project:read", "project:create", "account:*", "*:read"])
    }

    #[tokio::test]
    #[serial]
    async fn test_ctx_extend() -> CoreResult<()> {
        let mut ctx = CoreCtx::bootstrap()?;

        ctx.extend_perms(&["account:create"])?;

        let checker = ctx.permission_checker();
        let perms = PermissionRule::perms_from_str_slice(&["account:create"])?;
        let res = checker.has_subset(&perms);

        assert_eq!(res, true, "ctx should have account:create permission");

        Ok(())
    }
}
