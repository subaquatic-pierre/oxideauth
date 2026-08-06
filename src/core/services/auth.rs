use std::{collections::HashSet, sync::Arc};

use uuid::Uuid;

use crate::{
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::permission::{PermissionCheck, PermissionChecker},
        services::account::AccountService,
    },
    store::{ctx::StoreCtx, dbx::PgDbx, manager::StoreManager, traits::dbx::DbExecutor},
};

pub struct AuthService<D>
where
    D: DbExecutor,
{
    acc_svc: AccountService<D>,
}

impl<D: DbExecutor> AuthService<D> {
    pub fn new(acc_svc: AccountService<D>) -> Self {
        Self { acc_svc }
    }

    pub async fn register_account(&self, ctx: &CoreCtx) -> CoreResult<()> {
        Ok(())
    }

    pub async fn black_list_token(&self, ctx: &CoreCtx) -> CoreResult<()> {
        Ok(())
    }

    pub async fn revoke_token(&self, ctx: &CoreCtx) -> CoreResult<()> {
        Ok(())
    }

    pub async fn refresh_token(&self, ctx: &CoreCtx) -> CoreResult<()> {
        Ok(())
    }

    pub async fn request_token(&self, ctx: &CoreCtx) -> CoreResult<()> {
        Ok(())
    }
}

pub struct AuthValidator<'a> {
    ctx: &'a CoreCtx,
}

impl<'a> AuthValidator<'a> {
    pub fn new(ctx: &'a CoreCtx) -> Self {
        Self { ctx }
    }

    pub fn validate_perms<'b>(granted: &PermissionChecker, required: &[&str]) -> CoreResult<()> {
        let required = PermissionCheck::perms_from_str_slice(required)?;
        let all_required_match_granted = granted.has_subset(&required);
        if (all_required_match_granted) {
            Ok(())
        } else {
            Err(CoreError::Auth("invalid permissions".to_string()))
        }
    }

    pub fn validate_ctx_perms<'b>(&self, required: &[&str]) -> CoreResult<()> {
        let required = PermissionCheck::perms_from_str_slice(required)?;
        let granted = self.ctx.permission_checker();

        let all_required_match_granted = granted.has_subset(&required);
        if (all_required_match_granted) {
            Ok(())
        } else {
            Err(CoreError::Auth(format!(
                "invalid permissions, required premissions: {}",
                required
                    .iter()
                    .map(|el| el.to_string())
                    .collect::<Vec<String>>()
                    .join(",")
            )))
        }
    }

    /// Validates the requested workspace ID against the user's operational context.
    ///
    /// This function enforces the separation of tenancy by ensuring that a user
    /// operating within a scoped context (i.e., not a global/root user) can only
    /// query or mutate data within their assigned workspace.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The current operational context (`CoreCtx`), which holds the user's
    ///          authentication and assigned workspace scope.
    /// * `requested_workspace_id`: The optional workspace ID provided by the client
    ///                             (e.g., in a query filter or mutation DTO).
    ///
    /// # Behavior
    ///
    /// 1. **Global Context (Admin/Root):** If `ctx.is_global_workspace()` is true,
    ///    validation passes immediately, and the `StoreCtx` is returned.
    ///
    /// 2. **Scoped Context (Tenant User):**
    ///    * **Required:** If `requested_workspace_id` is `None`, an error is returned.
    ///    * **Authorization:** The provided `requested_workspace_id` must exactly match
    ///      the workspace ID stored in the `ctx.workspace_id()`.
    ///
    /// # Returns
    ///
    /// A `CoreResult<StoreCtx>` containing:
    ///
    /// * `Ok(StoreCtx)`: If validation succeeds a StoreCtx is created from CoreCtx.
    /// * `Err(CoreError::Auth)`: If the user is scoped and fails the validation checks.
    pub fn scope_store_workspace(
        &self,
        requested_workspace_id: Option<Uuid>,
    ) -> CoreResult<StoreCtx> {
        let ctx = self.ctx;
        let mut store_ctx: StoreCtx = ctx.into();
        // set workspace context
        if let Some(workspace_id) = self.validate_workspace(requested_workspace_id)? {
            store_ctx.set_workspace_scope(requested_workspace_id);
        }
        Ok(store_ctx)
    }

    /// Validates the requested workspace ID against the user's operational context.
    ///
    /// This function enforces the separation of tenancy by ensuring that a user
    /// operating within a scoped context (i.e., not a global/root user) can only
    /// query or mutate data within their assigned workspace.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The current operational context (`CoreCtx`), which holds the user's
    ///          authentication and assigned workspace scope.
    /// * `requested_workspace_id`: The optional workspace ID provided by the client
    ///                             (e.g., in a query filter or mutation DTO).
    ///
    /// # Behavior
    ///
    /// 1. **Global Context (Admin/Root):** If `ctx.is_global_workspace()` is true,
    ///    validation passes immediately, and the `requested_workspace_id` is returned
    ///    as is (it may be `None`).
    ///
    /// 2. **Scoped Context (Tenant User):**
    ///    * **Required:** If `requested_workspace_id` is `None`, an error is returned.
    ///    * **Authorization:** The provided `requested_workspace_id` must exactly match
    ///      the workspace ID stored in the `ctx.workspace_id()`.
    ///
    /// # Returns
    ///
    /// A `CoreResult<Option<Uuid>>` containing:
    ///
    /// * `Ok(Some(Uuid))`: If validation succeeds (either global or matched scoped).
    /// * `Ok(None)`: Only if in a global context and no ID was requested.
    /// * `Err(CoreError::Auth)`: If the user is scoped and fails the validation checks.
    pub fn validate_workspace(
        &self,
        requested_workspace_id: Option<Uuid>,
    ) -> CoreResult<Option<Uuid>> {
        let ctx = self.ctx;
        let is_global_context = ctx.is_global_workspace()?;

        if is_global_context {
            // Case 1: Global context (admin/root).
            return Ok(requested_workspace_id);
        }

        // Case 2: Scoped context.
        let ctx_workspace_id = ctx.workspace_id();

        // 2a: Scoped user must provide an ID.
        let requested_workspace_id = match requested_workspace_id {
            Some(id) => id,
            None => return Err(CoreError::Auth("workspace_id required".to_string())),
        };

        // 2b: Provided ID must match the context's ID.
        if ctx_workspace_id != requested_workspace_id {
            return Err(CoreError::Auth("unauthorized workspace".to_string()));
        }

        // 2c: Success. The scoped user is operating within their assigned workspace.
        Ok(Some(requested_workspace_id))
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
    async fn test_validate_perms() -> CoreResult<()> {
        let app = init_test().await;
        let granted = setup_checker()?;

        let mut ctx = CoreCtx::new_test()?;
        ctx.extend_perms(&["account:create"])?;
        let auth = AuthValidator::new(&ctx);

        let success = auth.validate_ctx_perms(&["account:create"]);

        assert!(
            matches!(success, Ok(())),
            "should be success on validate context"
        );
        Ok(())
    }
}
