use std::str::FromStr;
use std::{collections::HashSet, ops::Deref};

use time::OffsetDateTime;
use uuid::Uuid;

use crate::dev::fixtures::{global_ws_id, root_user_id};
use crate::{
    core::{
        error::CoreResult,
        models::{
            account::Account,
            membership::CachedMembership,
            permission::{PermissionCheck, PermissionChecker},
            workspace::{Workspace, GLOBAL_WS_ID},
        },
    },
    store::ctx::StoreCtx,
    utils::time::now_utc,
};

#[derive(Clone, Debug)]
pub struct CoreCtx {
    cached_mem: CachedMembership,
    account: Account,
    workspace: Workspace,
    perm_checker: PermissionChecker,
}

impl CoreCtx {
    pub fn new(
        cached_mem: CachedMembership,
        account: Account,
        workspace: Workspace,
    ) -> CoreResult<Self> {
        let perm_checker = PermissionChecker::from_string_vec(cached_mem.permissions.clone())?;
        Ok(Self {
            cached_mem,
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

        let mut cm = CachedMembership::default();
        cm.workspace_id = global_ws_id();

        let perm_checker = PermissionChecker::from_string_vec(cm.permissions.clone())?;
        Ok(Self {
            cached_mem: cm,
            account: acc,
            workspace: ns,
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

    pub fn workspace_id(&self) -> Uuid {
        self.cached_mem.workspace_id
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
