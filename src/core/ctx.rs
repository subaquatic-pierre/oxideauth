use uuid::Uuid;

use crate::cache::entities::auth::AuthCache;
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
/// acts on. For scoped tokens it equals the token's workspace. For global/root
/// tokens the middleware sets it from the `X-Workspace-Id` header.
#[derive(Clone, Debug)]
pub struct CoreCtx {
    pub(crate) auth_cache: AuthCache,
    scoped_ws_id: Uuid,
    perm_checker: PermissionEngine,
}

impl CoreCtx {
    pub fn new(auth_cache: AuthCache, scoped_ws_id: Uuid) -> CoreResult<Self> {
        let perm_checker =
            PermissionEngine::from_string_vec(auth_cache.auth_scope.permissions.clone())?;
        Ok(Self {
            auth_cache,
            scoped_ws_id,
            perm_checker,
        })
    }

    pub fn new_root() -> CoreResult<Self> {
        let auth_cache = AuthCache::root_cache();
        let perm_checker = PermissionEngine::from_string_vec(vec![])?;
        Ok(Self {
            auth_cache,
            scoped_ws_id: Uuid::nil(),
            perm_checker,
        })
    }

    /// Creates a bootstrap context with nil UUIDs — no pre-existing DB data required.
    ///
    /// Used during seeding/initialization before any accounts or workspaces exist.
    /// Has `*:*` permissions and nil scoped workspace (no row-level filtering).
    pub fn bootstrap() -> CoreResult<Self> {
        let auth_cache = AuthCache::bootstrap_cache();
        let perm_checker = PermissionEngine::from_string_vec(vec!["*:*".to_string()])?;
        Ok(Self {
            auth_cache,
            scoped_ws_id: Uuid::nil(),
            perm_checker,
        })
    }

    /// Creates a minimal system-level context scoped to a specific workspace.
    ///
    /// Used by unauthenticated flows (registration, login, password reset, OAuth)
    /// that need a properly scoped `StoreCtx` without a pre-existing user session.
    pub fn system(workspace_id: Uuid) -> CoreResult<Self> {
        let perm_checker = PermissionEngine::from_string_vec(vec![])?;
        let auth_cache = AuthCache::root_cache();
        Ok(Self {
            auth_cache,
            scoped_ws_id: workspace_id,
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

    /// Whether the token is scoped to the global/root workspace (admin).
    pub fn is_global_workspace(&self) -> CoreResult<bool> {
        Ok(self.auth_cache.auth_scope.workspace_slug == "global")
    }

    // --- Operational target (may differ from token scope for root tokens) ---

    /// The workspace this request actually operates on.
    ///
    /// Set during context resolution to the token's workspace for scoped users,
    /// or to the `X-Workspace-Id` header value for global/root tokens.
    pub fn scoped_ws_id(&self) -> Uuid {
        self.scoped_ws_id
    }

    /// Override the operational workspace target (used by middleware for
    /// global tokens that supply an `X-Workspace-Id` header).
    pub fn set_scoped_ws_id(&mut self, ws_id: Uuid) {
        self.scoped_ws_id = ws_id;
    }
}

impl From<CoreCtx> for StoreCtx {
    fn from(ctx: CoreCtx) -> Self {
        Self::new(ctx.auth_cache.acc_id, ctx.scoped_ws_id)
    }
}

impl From<&CoreCtx> for StoreCtx {
    fn from(ctx: &CoreCtx) -> Self {
        Self::new(ctx.auth_cache.acc_id, ctx.scoped_ws_id)
    }
}

impl From<&mut CoreCtx> for StoreCtx {
    fn from(ctx: &mut CoreCtx) -> Self {
        Self::new(ctx.auth_cache.acc_id, ctx.scoped_ws_id)
    }
}

/// Resolves and caches the UUIDs of well-known entities (global workspace, root
/// account) so that context factories like `new_root()` and `system()` use real
/// DB-backed identities instead of hardcoded strings.
///
/// # Lifecycle
///
/// 1. Created empty at startup.
/// 2. Initialized via `init_from_seed()` after `seed_all` creates the global
///    workspace and root account.
/// 3. From that point on, `new_root()` and `system()` return contexts with real
///    UUIDs.
pub struct ContextFactory {
    global_ws_id: OnceLock<Uuid>,
    root_user_id: OnceLock<Uuid>,
}

impl ContextFactory {
    /// Creates an empty factory. Call `init_from_seed()` before using
    /// `new_root()` or `system()` in production.
    pub fn new() -> Self {
        Self {
            global_ws_id: OnceLock::new(),
            root_user_id: OnceLock::new(),
        }
    }

    /// Looks up the global workspace (by slug `"global"`) and the root account
    /// (by email `"root@system.local"`) in the database and caches their UUIDs.
    ///
    /// Must be called after `seed_all` has completed.
    pub async fn init_from_seed<D: DbExecutor>(&self, sm: &StoreManager<D>) -> CoreResult<()> {
        let store_ctx = StoreCtx::bootstrap();

        // Look up global workspace by slug
        let global_ws = sm
            .workspace
            .get_by_slug_opt(&store_ctx, "global")
            .await?
            .map(|ws| Uuid::from(ws.id))
            .ok_or_else(|| {
                CoreError::InvalidParams(
                    "global workspace not found — run seed_all first".to_string(),
                )
            })?;
        self.global_ws_id.set(global_ws).map_err(|_| {
            CoreError::InvalidParams("ContextFactory already initialized".to_string())
        })?;

        // Look up root account by email
        let root_user = sm
            .account
            .get_by_email(&store_ctx, "root@system.local")
            .await?
            .map(|acc| Uuid::from(acc.id))
            .ok_or_else(|| {
                CoreError::InvalidParams("root account not found — run seed_all first".to_string())
            })?;
        self.root_user_id.set(root_user).map_err(|_| {
            CoreError::InvalidParams("ContextFactory already initialized".to_string())
        })?;

        Ok(())
    }

    /// Returns the cached global workspace UUID.
    pub fn global_ws_id(&self) -> Uuid {
        *self.global_ws_id.get().unwrap_or(&Uuid::nil())
    }

    /// Returns the cached root account UUID.
    pub fn root_user_id(&self) -> Uuid {
        *self.root_user_id.get().unwrap_or(&Uuid::nil())
    }

    /// Creates a root-level `CoreCtx` using cached real UUIDs.
    ///
    /// Falls back to nil UUIDs if `init_from_seed()` hasn't been called yet
    /// (e.g., during tests that don't go through `seed_all`).
    pub fn new_root(&self) -> CoreResult<CoreCtx> {
        CoreCtx::system(self.global_ws_id())
    }

    /// Creates a system-level `CoreCtx` scoped to the given workspace, using
    /// cached root identity UUIDs.
    pub fn system(&self, workspace_id: Uuid) -> CoreResult<CoreCtx> {
        CoreCtx::system(workspace_id)
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
        let mut ctx = CoreCtx::new_root()?;

        ctx.extend_perms(&["account:create"])?;

        let checker = ctx.permission_checker();
        let perms = PermissionRule::perms_from_str_slice(&["account:create"])?;
        let res = checker.has_subset(&perms);

        assert_eq!(res, true, "ctx should have account:create permission");

        Ok(())
    }
}
