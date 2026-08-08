use std::str::FromStr;
use std::{collections::HashSet, ops::Deref};

use time::OffsetDateTime;
use uuid::Uuid;

use crate::cache::entities::auth::AuthCache;
use crate::dev::fixtures::{global_ws_id, root_user_id};
use crate::{
    core::{
        error::CoreResult,
        models::{
            account::Account,
            membership::MembershipCache,
            permission::{PermissionCheck, PermissionChecker},
            workspace::{GLOBAL_WS_ID, Workspace},
        },
    },
    store::ctx::StoreCtx,
    utils::time::now_utc,
};

#[derive(Clone, Debug)]
pub struct CoreCtx {
    cached_mem: MembershipCache,
    auth_cache: AuthCache,
    account: Account,
    workspace: Workspace,
    perm_checker: PermissionChecker,
}

impl CoreCtx {
    pub fn new(
        cached_mem: MembershipCache,
        auth_cache: AuthCache,
        account: Account,
        workspace: Workspace,
    ) -> CoreResult<Self> {
        let perm_checker = PermissionChecker::from_string_vec(cached_mem.permissions.clone())?;
        Ok(Self {
            cached_mem,
            auth_cache,
            account,
            workspace,
            perm_checker,
        })
    }

    pub fn new_test() -> CoreResult<Self> {
        let mut acc = Account::default();
        acc.id = root_user_id();
        let mut ns = Workspace::default();
        ns.id = global_ws_id();

        let mut mem_cache = MembershipCache::default();
        mem_cache.workspace_id = global_ws_id();

        let perm_checker = PermissionChecker::from_string_vec(mem_cache.permissions.clone())?;
        Ok(Self {
            auth_cache: AuthCache::root_cache(),
            cached_mem: mem_cache,
            account: acc,
            workspace: ns,
            perm_checker,
        })
    }

    /// Creates a minimal system-level context scoped to a specific workspace.
    ///
    /// Used by unauthenticated flows (registration, login, password reset, OAuth)
    /// that need a properly scoped `StoreCtx` without a pre-existing user session.
    /// The system account ID is `Uuid::nil()` and permissions start empty —
    /// callers must use `extend_perms()` to grant operation-specific permissions.
    pub fn system(workspace_id: Uuid) -> CoreResult<Self> {
        let system_account_id = Uuid::nil();
        let mut acc = Account::default();
        acc.id = system_account_id;
        let mut ws = Workspace::default();
        ws.id = workspace_id;
        let mut cm = MembershipCache::default();
        cm.workspace_id = workspace_id;
        let perm_checker = PermissionChecker::from_string_vec(vec![])?;
        let auth_cache = AuthCache::root_cache();
        Ok(Self {
            cached_mem: cm,
            auth_cache,
            account: acc,
            workspace: ws,
            perm_checker,
        })
    }

    pub fn permission_checker(&self) -> &PermissionChecker {
        &self.perm_checker
    }

    pub fn extend_perms(&mut self, perms: &[&str]) -> CoreResult<()> {
        let new_perms: Vec<String> = perms.iter().map(|el| el.to_string()).collect();

        let mut new_perms: Vec<PermissionCheck> = PermissionCheck::perms_from_str_slice(perms)?;

        self.perm_checker.extend(new_perms);
        Ok(())
    }

    pub fn account_id(&self) -> Uuid {
        self.account.id
    }

    pub fn workspace_id(&self) -> Uuid {
        self.cached_mem.workspace_id
    }

    pub fn membership_id(&self) -> Uuid {
        self.cached_mem.id
    }

    pub fn set_workspace_id(&mut self, ws_id: Uuid) {
        self.cached_mem.workspace_id = ws_id;
        self.workspace.id = ws_id;
    }

    pub fn is_global_workspace(&self) -> CoreResult<bool> {
        Ok(self.workspace.id == global_ws_id())
    }
}

impl From<CoreCtx> for StoreCtx {
    fn from(ctx: CoreCtx) -> Self {
        Self::new(ctx.account.id, ctx.workspace.id)
    }
}

impl From<&CoreCtx> for StoreCtx {
    fn from(ctx: &CoreCtx) -> Self {
        Self::new(ctx.account.id, ctx.workspace.id)
    }
}

impl From<&mut CoreCtx> for StoreCtx {
    fn from(ctx: &mut CoreCtx) -> Self {
        Self::new(ctx.account.id, ctx.workspace.id)
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use crate::dev::init::init_test;

    use super::*;

    fn setup_checker() -> CoreResult<PermissionChecker> {
        PermissionChecker::from_str_slice(&[
            "project:read",
            "project:create",
            "account:*",
            "*:read",
        ])
    }

    #[tokio::test]
    #[serial]
    async fn test_ctx_extend() -> CoreResult<()> {
        let mut ctx = CoreCtx::new_test()?;

        let initial_perms = ctx.permission_checker();

        ctx.extend_perms(&["account:create"])?;

        let checker = ctx.permission_checker();
        let perms = PermissionCheck::perms_from_str_slice(&["account:create"])?;
        let res = checker.has_subset(&perms);

        assert_eq!(res, true, "ctx should have account:create permission");

        Ok(())
    }
}
