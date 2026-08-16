use uuid::Uuid;

use crate::{
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::permission::{PermissionEngine, PermissionRule},
    },
    store::ctx::StoreCtx,
};

pub struct AuthValidator<'a> {
    ctx: &'a CoreCtx,
}

impl<'a> AuthValidator<'a> {
    pub fn new(ctx: &'a CoreCtx) -> Self {
        Self { ctx }
    }

    pub fn validate_perms<'b>(granted: &PermissionEngine, required: &[&str]) -> CoreResult<()> {
        let required = PermissionRule::perms_from_str_slice(required)?;
        let all_required_match_granted = granted.has_subset(&required);
        if (all_required_match_granted) {
            Ok(())
        } else {
            Err(CoreError::Auth("invalid permissions".to_string()))
        }
    }

    pub fn validate_ctx_perms<'b>(&self, required: &[&str]) -> CoreResult<()> {
        // info!("CTX in validate_ctx_perms: {:#?}", self.ctx);
        let required = PermissionRule::perms_from_str_slice(required)?;
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
    /// If authenticated in the system, then able to operate in any workspace
    /// If not system namespace authorized
    /// then requested_workspace_id match must self.ctx.ws_cache.id
    pub fn scope_store_workspace(
        &self,
        requested_workspace_id: Option<Uuid>,
    ) -> CoreResult<StoreCtx> {
        let ctx = self.ctx;
        let mut store_ctx: StoreCtx = ctx.into();
        // set workspace context to the validated/derived workspace
        if let Some(workspace_id) = self.validate_workspace(requested_workspace_id)? {
            store_ctx.set_workspace_scope(Some(workspace_id));
        }
        Ok(store_ctx)
    }

    /// Validates the requested workspace ID against the user's operational context.
    /// If not system namespace authorized
    /// then requested_workspace_id match must self.ctx.ws_cache.id
    pub fn validate_workspace(
        &self,
        requested_workspace_id: Option<Uuid>,
    ) -> CoreResult<Option<Uuid>> {
        let is_global_context = self.ctx.is_system_workspace()?;

        if is_global_context {
            // Case 1: system context
            return Ok(requested_workspace_id);
        }

        // Case 2: Scoped context — must have a concrete workspace.
        match requested_workspace_id {
            Some(id) => {
                if self.ctx.ws_cache.id != id {
                    return Err(CoreError::Auth("unauthorized workspace".to_string()));
                }
                Ok(Some(id))
            }
            None => {
                // Derive workspace from the context.
                Ok(Some(self.ctx.ws_cache.id))
            }
        }
    }
}
