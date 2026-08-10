use uuid::Uuid;

use crate::cache::entities::auth::AuthCache;
use crate::dev::fixtures::{global_ws_id, root_user_id};
use crate::{
    core::{
        error::CoreResult,
        models::{
            permission::{PermissionEngine, PermissionRule},
            workspace::Workspace,
        },
    },
    store::ctx::StoreCtx,
};

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

    pub fn new_test() -> CoreResult<Self> {
        let auth_cache = AuthCache::root_cache();
        let perm_checker = PermissionEngine::from_string_vec(vec![])?;
        Ok(Self {
            auth_cache,
            scoped_ws_id: global_ws_id(),
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
        let mut ctx = CoreCtx::new_test()?;

        ctx.extend_perms(&["account:create"])?;

        let checker = ctx.permission_checker();
        let perms = PermissionRule::perms_from_str_slice(&["account:create"])?;
        let res = checker.has_subset(&perms);

        assert_eq!(res, true, "ctx should have account:create permission");

        Ok(())
    }
}
