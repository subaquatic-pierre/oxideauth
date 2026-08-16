use uuid::Uuid;

use crate::{
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::permission::{PermissionEngine, PermissionRule, PermissionSet},
        services::policy::PolicyEngine,
    },
    store::{ctx::StoreCtx, stores::workspace::SYSTEM_CONST},
};

/// Central authorization controller, registered once in the [`ServiceRegistry`]
/// and shared across requests.
///
/// It holds the [`PermissionEngine`] and [`PolicyEngine`] and delegates
/// permission/policy validation to them. It holds no per-request state and
/// carries no lifetime or generic parameters — the request [`CoreCtx`] is
/// passed into each method.
#[derive(Clone, Debug, Default)]
pub struct AuthValidator {
    permission_engine: PermissionEngine,
    policy_engine: PolicyEngine,
}

impl AuthValidator {
    pub fn new() -> Self {
        Self {
            permission_engine: PermissionEngine,
            policy_engine: PolicyEngine,
        }
    }

    /// Validates `required` permissions against an explicit `granted` set
    /// (used for escalation flows that construct their own extended set).
    pub fn validate_perms(&self, granted: &PermissionSet, required: &[&str]) -> CoreResult<()> {
        let required = PermissionRule::perms_from_str_slice(required)?;
        if PermissionEngine::has_subset(granted, &required) {
            Ok(())
        } else {
            Err(CoreError::Auth("invalid permissions".to_string()))
        }
    }

    /// Validates `required` permissions against the caller's context-derived
    /// permission set, delegating to the [`PermissionEngine`].
    pub fn validate_ctx_perms(&self, ctx: &CoreCtx, required: &[&str]) -> CoreResult<()> {
        let required = PermissionRule::perms_from_str_slice(required)?;
        let granted = ctx.permissions();

        if PermissionEngine::has_subset(&granted, &required) {
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

    /// Builds a store context scoped to the validated/derived workspace.
    pub fn scope_store_workspace(
        &self,
        ctx: &CoreCtx,
        requested_workspace_id: Option<Uuid>,
    ) -> CoreResult<StoreCtx> {
        // NOTE(workspace-scope): scoped — set to the validated/derived workspace.
        let mut store_ctx: StoreCtx = ctx.into();
        // set workspace context to the validated/derived workspace
        if let Some(workspace_id) = self.validate_workspace(ctx, requested_workspace_id)? {
            store_ctx.set_workspace_scope(Some(workspace_id));
        }
        Ok(store_ctx)
    }

    /// Validates the requested workspace ID against the caller's scope.
    ///
    /// Cross-namespace operation is gated by system-namespace admin membership
    /// (FR-018): a caller whose authenticated scope is the system namespace and
    /// holds the `*:*` admin permission may operate on any requested workspace.
    /// All other callers must match (or derive from) their context workspace.
    pub fn validate_workspace(
        &self,
        ctx: &CoreCtx,
        requested_workspace_id: Option<Uuid>,
    ) -> CoreResult<Option<Uuid>> {
        let is_system_namespace_admin = ctx.auth_cache.auth_scope.workspace_slug
            == SYSTEM_CONST.system_ws_slug
            && ctx.permissions().has_global_wildcard();

        if is_system_namespace_admin {
            // Case 1: system-namespace admin — may operate in any workspace.
            return Ok(requested_workspace_id);
        }

        // Case 2: Scoped context — must have a concrete workspace.
        match requested_workspace_id {
            Some(id) => {
                if ctx.ws_cache.id != id {
                    return Err(CoreError::Auth("unauthorized workspace".to_string()));
                }
                Ok(Some(id))
            }
            None => {
                // Derive workspace from the context.
                Ok(Some(ctx.ws_cache.id))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::entities::{
        auth::{AuthCache, AuthScopeCache},
        workspace::WorkspaceCache,
    };
    use uuid::Uuid;

    fn ctx_with(workspace_slug: &str, permissions: Vec<String>, ws_id: Uuid) -> CoreCtx {
        let auth_cache = AuthCache {
            mem_id: Uuid::new_v4(),
            acc_id: Uuid::new_v4(),
            sid: None,
            mem_version: 0,
            acc_version: 0,
            mem_active: true,
            acc_enabled: true,
            auth_scope: AuthScopeCache {
                workspace_id: ws_id,
                workspace_slug: workspace_slug.to_string(),
                project_id: None,
                roles: vec![],
                permissions,
            },
        };
        let ws_cache = WorkspaceCache {
            id: ws_id,
            slug: workspace_slug.to_string(),
            ..WorkspaceCache::default()
        };
        CoreCtx::new(auth_cache, ws_cache).unwrap()
    }

    #[test]
    fn test_cross_namespace_gated_by_system_namespace_admin() -> CoreResult<()> {
        let validator = AuthValidator::new();
        let other = Uuid::new_v4();

        // System-namespace admin (slug "system", *:*) may scope anywhere (FR-018).
        let admin = ctx_with(SYSTEM_CONST.system_ws_slug, vec!["*:*".to_string()], Uuid::new_v4());
        assert_eq!(
            validator.validate_workspace(&admin, Some(other))?,
            Some(other)
        );

        // A non-system workspace admin holding *:* cannot cross namespaces.
        let acme = ctx_with("acme", vec!["*:*".to_string()], Uuid::new_v4());
        assert!(matches!(
            validator.validate_workspace(&acme, Some(other)),
            Err(CoreError::Auth(_))
        ));

        // The non-system admin CAN still operate within their own workspace.
        assert_eq!(
            validator.validate_workspace(&acme, Some(acme.ws_cache.id))?,
            Some(acme.ws_cache.id)
        );

        Ok(())
    }
}
