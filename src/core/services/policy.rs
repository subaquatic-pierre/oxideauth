//! Policy service (CRUD) and policy evaluation engine.

use std::sync::Arc;

use uuid::Uuid;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            list::ListResponse,
            permission::PermissionRule,
            policy::{
                Policy, PolicyCreateParams, PolicyDeleteParams, PolicyDescribeParams,
                PolicyListParams, PolicySet, PolicyUpdateParams, parse_constraint, runtime_key,
            },
        },
        services::{
            permission::CANONICAL_PERMISSIONS, validator::AuthValidator,
            workspace::WorkspaceService,
        },
        traits::{
            list::RequestListParams,
            service::{
                CoreModelCreateService, CoreModelDeleteService, CoreModelDescribeService,
                CoreModelListService, CoreModelService, CoreModelUpdateService,
            },
        },
    },
    store::{
        contains::FilterByContains,
        ctx::StoreCtx,
        entities::{id::DbId, membership::MembershipRow, policy::PolicyEffect, role::RoleRow},
        error::StoreError,
        join::GetManyToMany,
        manager::StoreManager,
        stores::policy::PolicyStore,
        traits::{crud::*, dbx::DbExecutor},
    },
};

// ============================================================================
// PolicyEngine
// ============================================================================

/// A stateless policy evaluation engine (default-deny).
///
/// Held by [`crate::core::services::validator::AuthValidator`]. Evaluates a
/// pre-resolved [`PolicySet`] for a single `action`/`resource`/`constraint`
/// triple; it holds no per-request state.
#[derive(Clone, Debug, Default)]
pub struct PolicyEngine;

impl PolicyEngine {
    /// Returns `true` only when the resolved effect is [`PolicyEffect::Allow`].
    ///
    /// Missing lookups (default-deny) and explicit `Deny` entries both
    /// evaluate to `false`.
    pub fn evaluate(
        &self,
        set: &PolicySet,
        action: &str,
        resource: &str,
        constraint: Option<&str>,
    ) -> bool {
        matches!(
            set.get(action, resource, constraint),
            Some(PolicyEffect::Allow)
        )
    }
}

// ============================================================================
// PolicyService
// ============================================================================

/// CRUD service for workspace-scoped policies.
///
/// Validates the AWS-like policy body on create/update and enforces
/// `runtime_key` uniqueness per workspace (FR-003, validated at the service
/// layer since the key is derived, not stored).
pub struct PolicyService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    ws_svc: Arc<WorkspaceService<D, C>>,
    validator: Arc<AuthValidator>,
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D, C> for PolicyService<D, C> {
    type CoreModel = Policy;
    type ServiceStore = PolicyStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.policy
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

impl<D: DbExecutor, C: CacheExecutor> PolicyService<D, C> {
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

    // --- Validation helpers (FR-001, FR-015) ---

    /// `actions` must be non-empty; each entry must match the `resource:action`
    /// (or `*`) form.
    fn validate_actions(actions: &[String]) -> CoreResult<()> {
        if actions.is_empty() {
            return Err(CoreError::InvalidParams(
                "actions must be non-empty".to_string(),
            ));
        }
        for action in actions {
            PermissionRule::try_from(action.as_str()).map_err(|err| {
                CoreError::InvalidParams(format!("invalid action '{action}': {err}"))
            })?;
        }
        Ok(())
    }

    /// `resource` must be `"self"`, a valid UUID, or `"*"`.
    fn validate_resource(resource: &str) -> CoreResult<()> {
        if resource == "self" || resource == "*" || Uuid::parse_str(resource).is_ok() {
            Ok(())
        } else {
            Err(CoreError::InvalidParams(format!(
                "resource must be 'self', a valid UUID, or '*'; got '{resource}'"
            )))
        }
    }

    /// `constraint`, if present, must parse against the DSL grammar
    /// (see `contracts/policy-document.md`).
    fn validate_constraint(constraint: Option<&str>) -> CoreResult<()> {
        if let Some(constraint) = constraint {
            parse_constraint(constraint)
                .map_err(|msg| CoreError::InvalidParams(format!("invalid constraint: {msg}")))?;
        }
        Ok(())
    }

    /// Validates the full policy body against the AWS-like field rules.
    fn validate_policy_body(
        effect: PolicyEffect,
        actions: &[String],
        resource: &str,
        constraint: Option<&str>,
    ) -> CoreResult<()> {
        let _ = effect; // effect is constrained to allow|deny by its type (FR-015)
        Self::validate_actions(actions)?;
        Self::validate_resource(resource)?;
        Self::validate_constraint(constraint)
    }

    /// Enforces `runtime_key` uniqueness within the workspace (FR-003).
    ///
    /// Lists every policy in the (already workspace-scoped) store context and
    /// compares its compiled runtime key against the candidate key. On update,
    /// `exclude_id` skips the policy's own row.
    async fn ensure_runtime_key_unique(
        &self,
        store_ctx: &StoreCtx,
        effect: PolicyEffect,
        actions: &[String],
        resource: &str,
        constraint: Option<&str>,
        exclude_id: Option<Uuid>,
    ) -> CoreResult<()> {
        let key = runtime_key(effect, actions, resource, constraint);
        let existing = self.store().list(store_ctx, None, None).await?;

        for policy in existing {
            if let Some(exclude_id) = exclude_id {
                if policy.id == exclude_id.into() {
                    continue;
                }
            }
            let other_key = runtime_key(
                policy.effect,
                &policy.actions,
                &policy.resource,
                policy.constraint_expr.as_deref(),
            );
            if other_key == key {
                return Err(CoreError::AlreadyExists(
                    "a policy with the same effect, actions, resource, and constraint already exists in this workspace"
                        .to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Invalidates the `oxauth:policy:{mem_id}` cache for every membership whose
    /// effective policy set includes the given policy — either directly
    /// (`membership_policy` join) or transitively through a role (`role_policy`
    /// join → `membership_role` join). Mirrors
    /// [`PermissionService::invalidate_memberships_for_permission`].
    async fn invalidate_memberships_for_policy(
        &self,
        store_ctx: &StoreCtx,
        policy_id: Uuid,
    ) -> CoreResult<()> {
        let policy_db_id: DbId = policy_id.into();

        // Memberships that attach the policy directly (`membership_policy`).
        let direct = self
            .sm
            .membership
            .list_containing_policies(store_ctx, vec![policy_db_id], None, None, None)
            .await?;

        // Memberships whose roles attach the policy (`role_policy` →
        // `membership_role`).
        let roles = self
            .sm
            .role
            .list_containing_policies(store_ctx, vec![policy_db_id], None, None, None)
            .await?;
        let via_roles: Vec<MembershipRow> = if roles.is_empty() {
            Vec::new()
        } else {
            let role_ids = roles.iter().map(|role| role.id).collect();
            self.sm
                .membership
                .list_containing_roles(store_ctx, role_ids, None, None, None)
                .await?
        };

        // A membership may appear in both paths — dedupe before invalidating.
        let mut seen = std::collections::HashSet::new();
        for membership in direct.into_iter().chain(via_roles) {
            let mem_id: Uuid = membership.id.into();
            if seen.insert(mem_id) {
                self.cm.policy.invalidate(mem_id).await?;
            }
        }

        Ok(())
    }

    /// Resolves the effective [`PolicySet`] for a membership (US4).
    ///
    /// Loads the union of (membership → roles → role_policy) and
    /// (membership → membership_policy) via the store joins added in US2, then
    /// compiles it into a [`PolicySet`] (deduped per
    /// `action|resource|constraint`, `Deny` wins on collision).
    ///
    /// The store context is unscoped (`StoreCtx::bootstrap`): the query is keyed
    /// by the membership id itself, so no workspace filter is required.
    pub async fn resolve_for_membership(&self, membership_id: Uuid) -> CoreResult<PolicySet> {
        let store_ctx = StoreCtx::bootstrap();
        let membership_db_id: DbId = membership_id.into();

        // membership -> roles
        let membership_with_roles = self
            .sm
            .membership
            .get_many_to_many(&store_ctx, &membership_db_id)
            .await?;

        // membership -> policies (directly attached)
        let membership_with_policies = self
            .sm
            .membership
            .get_many_to_many_policies(&store_ctx, &membership_db_id)
            .await?;

        // membership -> roles -> policies (via role_policy)
        let mut policies: Vec<Policy> = Vec::new();
        for role in membership_with_roles.roles {
            let role_with_policies = self
                .sm
                .role
                .get_many_to_many_policies(&store_ctx, &role.id)
                .await?;
            policies.extend(role_with_policies.policies.into_iter().map(Into::into));
        }

        // membership -> policies (directly attached)
        policies.extend(
            membership_with_policies
                .policies
                .into_iter()
                .map(Into::into),
        );

        Ok(PolicySet::from_policies(policies))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D, C> for PolicyService<D, C> {
    type CreateParams = PolicyCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.policy.create;

    async fn create(
        &self,
        ctx: &mut CoreCtx,
        params: Self::CreateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::CREATE_PERMISSION])
            .await?;

        // Validate the AWS-like policy body before persisting.
        Self::validate_policy_body(
            params.effect.clone(),
            &params.actions,
            &params.resource,
            params.constraint.as_deref(),
        )?;

        // Enforce runtime-key uniqueness within the workspace (FR-003).
        self.ensure_runtime_key_unique(
            &store_ctx,
            params.effect.clone(),
            &params.actions,
            &params.resource,
            params.constraint.as_deref(),
            None,
        )
        .await?;

        let row = store.create(&store_ctx, params.into()).await?;

        // A new policy cannot be attached to anything yet, so this is a no-op
        // in practice — kept for symmetry with update/delete invalidation.
        self.invalidate_memberships_for_policy(&store_ctx, row.id.into())
            .await?;

        Ok(row.into())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D, C> for PolicyService<D, C> {
    type DescribeParams = PolicyDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.policy.describe;

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DescribeParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::DESCRIBE_PERMISSION])
            .await?;

        let row = store.get(&store_ctx, &params.id.into()).await?;

        Ok(row.into())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D, C> for PolicyService<D, C> {
    type ListParams = PolicyListParams;
    const LIST_PERMISSION: &'static str = CANONICAL_PERMISSIONS.policy.list;

    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: Self::ListParams,
    ) -> CoreResult<ListResponse<Self::CoreModel>> {
        let store = self.store();
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::LIST_PERMISSION])
            .await?;

        let options = params.list_options();
        let tags_filter = params.validate_filter_tags()?;
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

        let policies = data.into_iter().map(Policy::from).collect();

        Ok(ListResponse::new(policies, total, options))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D, C> for PolicyService<D, C> {
    type UpdateParams = PolicyUpdateParams;
    const UPDATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.policy.update;

    async fn update(
        &self,
        ctx: &mut CoreCtx,
        params: Self::UpdateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::UPDATE_PERMISSION])
            .await?;

        // The update is a partial patch, so fetch the current row to validate the
        // merged post-update body and to compute the post-update runtime key.
        let current = store.get(&store_ctx, &params.id.into()).await?;

        let effect = params.effect.clone().unwrap_or(current.effect);
        let actions = params
            .actions
            .clone()
            .unwrap_or_else(|| current.actions.clone());
        let resource = params
            .resource
            .clone()
            .unwrap_or_else(|| current.resource.clone());
        let constraint = params
            .constraint
            .clone()
            .or(current.constraint_expr.clone());

        Self::validate_policy_body(effect.clone(), &actions, &resource, constraint.as_deref())?;

        // Enforce runtime-key uniqueness within the workspace (FR-003), excluding
        // the policy's own id.
        self.ensure_runtime_key_unique(
            &store_ctx,
            effect,
            &actions,
            &resource,
            constraint.as_deref(),
            Some(params.id),
        )
        .await?;

        let res = store
            .update(&store_ctx, &params.id.into(), params.into())
            .await?;

        // The policy body changed: every membership whose effective set includes
        // it now holds a stale `oxauth:policy:{mem_id}` entry.
        self.invalidate_memberships_for_policy(&store_ctx, res.id.into())
            .await?;

        Ok(res.into())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D, C> for PolicyService<D, C> {
    type DeleteParams = PolicyDeleteParams;
    const DELETE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.policy.delete;

    async fn delete(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DeleteParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::DELETE_PERMISSION])
            .await?;

        let to_delete = self
            .describe(
                ctx,
                PolicyDescribeParams {
                    id: params.id,
                    workspace_id: params.workspace_id,
                },
            )
            .await?;

        let res = match store.delete(&store_ctx, &to_delete.id.into()).await {
            Ok(res) => res,
            Err(StoreError::ConstraintViolation) => {
                return Err(CoreError::InvalidParams(format!(
                    "policy '{}' is still attached to one or more roles or memberships and cannot be deleted",
                    to_delete
                        .name
                        .as_deref()
                        .unwrap_or(&to_delete.id.to_string())
                )));
            }
            Err(err) => return Err(err.into()),
        };

        // The policy is gone: every membership whose effective set included it
        // now holds a stale `oxauth:policy:{mem_id}` entry.
        self.invalidate_memberships_for_policy(&store_ctx, res.id.into())
            .await?;

        Ok(res.into())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        cache::{
            entities::policy::PolicyCache, manager::CacheManager, mock::MockChx,
            traits::CacheEntity,
        },
        config::Config,
        core::services::registry::ServiceRegistry,
        store::{
            dbx::MockDbx,
            entities::{
                audit::AuditFields,
                id::DbId,
                membership::{
                    JoinedPolicyOnMembership, JoinedRoleOnMembership, MembershipMeta,
                    MembershipRow, MembershipScope, MembershipStatus, MembershipWithPolicies,
                    MembershipWithRoles,
                },
                policy::{PolicyMeta, PolicyRow},
                role::{JoinedPolicyOnRole, RoleMeta, RoleRow, RoleWithPolicies},
                workspace::WorkspaceRow,
            },
        },
    };
    use serial_test::serial;

    /// Builds a `PolicyRow` for the in-memory mock.
    fn policy_row(id: Uuid, ws_id: Uuid, name: Option<&str>) -> PolicyRow {
        PolicyRow {
            id: id.into(),
            workspace_id: ws_id,
            name: name.map(|s| s.to_string()),
            effect: PolicyEffect::Allow,
            principal_id: None,
            actions: vec!["membership:update".to_string()],
            resource: "self".to_string(),
            constraint_expr: None,
            description: None,
            tags: vec![],
            meta: PolicyMeta::default(),
            created_at: time::OffsetDateTime::UNIX_EPOCH,
            updated_at: None,
        }
    }

    /// Builds a `PolicyService` backed by an in-memory `MockDbx` + `MockChx`.
    fn mock_svc(dbx: MockDbx) -> Arc<PolicyService<MockDbx, MockChx>> {
        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(dbx)));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        svc_reg.policy.clone()
    }

    #[tokio::test]
    #[serial]
    async fn test_scoped_service_requires_workspace_id() -> CoreResult<()> {
        let svc = mock_svc(MockDbx::new());
        let mut ctx = CoreCtx::bootstrap()?;

        let res = svc
            .scope_and_validate(&mut ctx, None, &[CANONICAL_PERMISSIONS.policy.describe])
            .await;

        assert!(matches!(res, Err(CoreError::InvalidParams(_))));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_policy_create() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let policy_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // create -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // create -> ensure_runtime_key_unique -> store.list (none existing)
            .with_all::<PolicyRow>(vec![])
            // create -> store.create
            .with_one::<PolicyRow>(policy_row(policy_id, ws_id, Some("self-update")));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["policy:create"])?;

        let params = PolicyCreateParams {
            workspace_id: ws_id,
            name: Some("self-update".to_string()),
            effect: PolicyEffect::Allow,
            principal_id: None,
            actions: vec!["membership:update".to_string()],
            resource: "self".to_string(),
            constraint: Some("membership.account.id === user.id".to_string()),
            description: None,
            tags: vec![],
            meta: PolicyMeta::default(),
        };

        let policy = svc.create(&mut ctx, params).await?;

        assert_eq!(policy.id, policy_id);
        assert_eq!(policy.workspace_id, ws_id);
        assert_eq!(policy.name.as_deref(), Some("self-update"));
        assert_eq!(policy.actions, vec!["membership:update".to_string()]);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_policy_create_duplicate_runtime_key_conflicts() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let existing_id = Uuid::new_v4();
        let new_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // create -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // create -> ensure_runtime_key_unique -> store.list (one existing policy)
            .with_all::<PolicyRow>(vec![policy_row(existing_id, ws_id, Some("existing"))]);
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["policy:create"])?;

        let params = PolicyCreateParams {
            workspace_id: ws_id,
            name: Some("new".to_string()),
            effect: PolicyEffect::Allow,
            principal_id: None,
            actions: vec!["membership:update".to_string()],
            resource: "self".to_string(),
            constraint: None,
            description: None,
            tags: vec![],
            meta: PolicyMeta::default(),
        };

        let res = svc.create(&mut ctx, params).await;
        let _ = new_id;

        assert!(matches!(res, Err(CoreError::AlreadyExists(_))));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_policy_create_invalid_actions_rejected() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();

        let dbx = MockDbx::new().with_optional::<WorkspaceRow>(Some(WorkspaceRow {
            id: ws_id.into(),
            ..Default::default()
        }));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["policy:create"])?;

        let params = PolicyCreateParams {
            workspace_id: ws_id,
            name: Some("bad".to_string()),
            effect: PolicyEffect::Allow,
            principal_id: None,
            actions: vec!["no-delimiter".to_string()],
            resource: "self".to_string(),
            constraint: None,
            description: None,
            tags: vec![],
            meta: PolicyMeta::default(),
        };

        let res = svc.create(&mut ctx, params).await;
        assert!(matches!(res, Err(CoreError::InvalidParams(_))));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_policy_create_invalid_resource_rejected() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();

        let dbx = MockDbx::new().with_optional::<WorkspaceRow>(Some(WorkspaceRow {
            id: ws_id.into(),
            ..Default::default()
        }));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["policy:create"])?;

        let params = PolicyCreateParams {
            workspace_id: ws_id,
            name: Some("bad".to_string()),
            effect: PolicyEffect::Allow,
            principal_id: None,
            actions: vec!["membership:update".to_string()],
            resource: "not-a-valid-resource".to_string(),
            constraint: None,
            description: None,
            tags: vec![],
            meta: PolicyMeta::default(),
        };

        let res = svc.create(&mut ctx, params).await;
        assert!(matches!(res, Err(CoreError::InvalidParams(_))));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_policy_create_invalid_constraint_rejected() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();

        let dbx = MockDbx::new().with_optional::<WorkspaceRow>(Some(WorkspaceRow {
            id: ws_id.into(),
            ..Default::default()
        }));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["policy:create"])?;

        let params = PolicyCreateParams {
            workspace_id: ws_id,
            name: Some("bad".to_string()),
            effect: PolicyEffect::Allow,
            principal_id: None,
            actions: vec!["membership:update".to_string()],
            resource: "self".to_string(),
            constraint: Some("not a valid constraint".to_string()),
            description: None,
            tags: vec![],
            meta: PolicyMeta::default(),
        };

        let res = svc.create(&mut ctx, params).await;
        assert!(matches!(res, Err(CoreError::InvalidParams(_))));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_policy_describe() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let policy_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // describe -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // describe -> store.get
            .with_optional::<PolicyRow>(Some(policy_row(policy_id, ws_id, Some("self-update"))));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["policy:describe"])?;

        let policy = svc
            .describe(
                &mut ctx,
                PolicyDescribeParams {
                    id: policy_id,
                    workspace_id: ws_id,
                },
            )
            .await?;

        assert_eq!(policy.id, policy_id);
        assert_eq!(policy.workspace_id, ws_id);
        assert_eq!(policy.name.as_deref(), Some("self-update"));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_policy_list() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let policy_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // list -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // list -> list_with_tags_and_filter
            .with_all::<PolicyRow>(vec![policy_row(policy_id, ws_id, Some("dev"))])
            // list -> count_with_tags_and_filter
            .with_one::<(i64,)>((1,));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["policy:list"])?;

        let res = svc
            .list(
                &mut ctx,
                PolicyListParams {
                    workspace_id: ws_id,
                    filter: None,
                    options: None,
                },
            )
            .await?;

        assert_eq!(res.data.len(), 1);
        assert_eq!(res.data[0].id, policy_id);
        assert_eq!(res.metadata.total, 1);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_policy_update() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let policy_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // update -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // update -> store.get (current row)
            .with_optional::<PolicyRow>(Some(policy_row(policy_id, ws_id, Some("self-update"))))
            // update -> ensure_runtime_key_unique -> store.list (only itself)
            .with_all::<PolicyRow>(vec![policy_row(policy_id, ws_id, Some("self-update"))])
            // update -> store.update
            .with_optional::<PolicyRow>(Some(policy_row(policy_id, ws_id, Some("renamed"))));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["policy:update"])?;

        let params = PolicyUpdateParams {
            id: policy_id,
            workspace_id: ws_id,
            name: Some("renamed".to_string()),
            effect: None,
            principal_id: None,
            actions: None,
            resource: None,
            constraint: None,
            description: None,
            tags: None,
            meta: None,
        };

        let updated = svc.update(&mut ctx, params).await?;

        assert_eq!(updated.id, policy_id);
        assert_eq!(updated.name.as_deref(), Some("renamed"));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_policy_update_invalidates_policy_cache_for_affected_memberships() -> CoreResult<()>
    {
        let ws_id = Uuid::new_v4();
        let policy_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // update -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // update -> store.get (current row)
            .with_optional::<PolicyRow>(Some(policy_row(policy_id, ws_id, Some("self-update"))))
            // update -> ensure_runtime_key_unique -> store.list (only itself)
            .with_all::<PolicyRow>(vec![policy_row(policy_id, ws_id, Some("self-update"))])
            // update -> store.update
            .with_optional::<PolicyRow>(Some(policy_row(policy_id, ws_id, Some("renamed"))))
            // invalidate_memberships_for_policy -> membership.list_containing_policies
            //   (membership holds the policy directly via membership_policy)
            .with_all::<MembershipRow>(vec![membership_row(mem_id, ws_id)])
            // invalidate_memberships_for_policy -> role.list_containing_policies (none)
            .with_all::<RoleRow>(vec![])
            // describe -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // describe -> store.get
            .with_optional::<PolicyRow>(Some(policy_row(policy_id, ws_id, Some("renamed"))));

        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(dbx)));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm.clone());

        // Seed the policy cache entry for the affected membership.
        let set = PolicySet::from_policies(vec![core_policy(
            PolicyEffect::Allow,
            vec!["membership:update"],
            "self",
            None,
        )]);
        cm.policy
            .write(&PolicyCache::new(mem_id, set), None)
            .await
            .unwrap();

        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["policy:update", "policy:describe"])?;

        let params = PolicyUpdateParams {
            id: policy_id,
            workspace_id: ws_id,
            name: Some("renamed".to_string()),
            effect: None,
            principal_id: None,
            actions: None,
            resource: None,
            constraint: None,
            description: None,
            tags: None,
            meta: None,
        };

        let updated = svc_reg.policy.update(&mut ctx, params).await?;

        assert_eq!(updated.id, policy_id);
        assert_eq!(updated.name.as_deref(), Some("renamed"));

        // The affected membership's policy cache entry must be gone.
        let fetched = cm.policy.fetch(&PolicyCache::new_key(mem_id)).await?;
        assert!(fetched.is_none(), "policy cache entry must be invalidated");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_policy_update_conflicting_runtime_key_rejected() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let policy_id = Uuid::new_v4();
        let other_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // update -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // update -> store.get (current row: allow|membership:update|self|)
            .with_optional::<PolicyRow>(Some(policy_row(policy_id, ws_id, Some("self-update"))))
            // update -> ensure_runtime_key_unique -> store.list (another identical policy)
            .with_all::<PolicyRow>(vec![policy_row(other_id, ws_id, Some("other"))]);
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["policy:update"])?;

        let params = PolicyUpdateParams {
            id: policy_id,
            workspace_id: ws_id,
            name: Some("renamed".to_string()),
            effect: None,
            principal_id: None,
            actions: None,
            resource: None,
            constraint: None,
            description: None,
            tags: None,
            meta: None,
        };

        let res = svc.update(&mut ctx, params).await;

        assert!(matches!(res, Err(CoreError::AlreadyExists(_))));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_policy_delete() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let policy_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // delete -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // delete -> describe -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // delete -> describe -> store.get
            .with_optional::<PolicyRow>(Some(policy_row(policy_id, ws_id, Some("self-update"))))
            // delete -> store.delete
            .with_optional::<PolicyRow>(Some(policy_row(policy_id, ws_id, Some("self-update"))));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["policy:delete", "policy:describe"])?;

        let deleted = svc
            .delete(
                &mut ctx,
                PolicyDeleteParams {
                    id: policy_id,
                    workspace_id: ws_id,
                },
            )
            .await?;

        assert_eq!(deleted.id, policy_id);
        assert_eq!(deleted.name.as_deref(), Some("self-update"));

        Ok(())
    }

    // --- PolicyEngine (US4) ---

    /// Builds a core `Policy` for engine/set tests.
    fn core_policy(
        effect: PolicyEffect,
        actions: Vec<&str>,
        resource: &str,
        constraint: Option<&str>,
    ) -> Policy {
        Policy {
            id: Uuid::new_v4(),
            effect,
            actions: actions.into_iter().map(|s| s.to_string()).collect(),
            resource: resource.to_string(),
            constraint: constraint.map(|s| s.to_string()),
            ..Policy::default()
        }
    }

    #[test]
    fn test_policy_engine_evaluate_default_deny() {
        let engine = PolicyEngine;
        let set = PolicySet::from_policies(vec![
            core_policy(
                PolicyEffect::Allow,
                vec!["membership:update"],
                "self",
                Some("membership.account.id === user.id"),
            ),
            core_policy(PolicyEffect::Deny, vec!["membership:delete"], "self", None),
        ]);

        assert!(
            engine.evaluate(
                &set,
                "membership:update",
                "self",
                Some("membership.account.id === user.id")
            ),
            "allow match must evaluate to true"
        );
        assert!(
            !engine.evaluate(&set, "membership:delete", "self", None),
            "explicit deny must evaluate to false"
        );
        assert!(
            !engine.evaluate(&set, "membership:update", "self", None),
            "constraint mismatch (default-deny) must evaluate to false"
        );
        assert!(
            !engine.evaluate(&set, "unknown:action", "self", None),
            "missing lookup (default-deny) must evaluate to false"
        );
    }

    // --- resolve_for_membership (US4) ---

    /// Builds a `MembershipRow` for the in-memory mock.
    fn membership_row(mem_id: Uuid, ws_id: Uuid) -> MembershipRow {
        MembershipRow {
            id: DbId::from(mem_id),
            account_id: Uuid::new_v4(),
            workspace_id: ws_id,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            profile_id: None,
            project_id: None,
            version: 1,
            tags: vec![],
            meta: MembershipMeta::default(),
            audit: AuditFields::default(),
        }
    }

    /// Builds a `role_policy`-joined policy row.
    fn joined_policy_on_role(
        ws_id: Uuid,
        name: &str,
        effect: PolicyEffect,
        constraint: Option<&str>,
    ) -> JoinedPolicyOnRole {
        JoinedPolicyOnRole {
            id: DbId::from(Uuid::new_v4()),
            workspace_id: ws_id,
            name: Some(name.to_string()),
            effect,
            principal_id: None,
            actions: vec!["profile:update".to_string()],
            resource: "self".to_string(),
            constraint_expr: constraint.map(|s| s.to_string()),
            description: None,
            tags: vec!["system".to_string()],
            meta: PolicyMeta::default(),
            created_at: time::OffsetDateTime::UNIX_EPOCH,
            updated_at: None,
        }
    }

    /// Builds a `membership_policy`-joined policy row.
    fn joined_policy_on_membership(
        ws_id: Uuid,
        name: &str,
        effect: PolicyEffect,
        constraint: Option<&str>,
    ) -> JoinedPolicyOnMembership {
        JoinedPolicyOnMembership {
            id: DbId::from(Uuid::new_v4()),
            workspace_id: ws_id,
            name: Some(name.to_string()),
            effect,
            principal_id: None,
            actions: vec!["membership:update".to_string()],
            resource: "self".to_string(),
            constraint_expr: constraint.map(|s| s.to_string()),
            description: None,
            tags: vec!["system".to_string()],
            meta: PolicyMeta::default(),
            created_at: time::OffsetDateTime::UNIX_EPOCH,
            updated_at: None,
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_for_membership_union_deny_wins() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // resolve_for_membership -> membership.get_many_to_many
            //   count_many guard + joined row (membership -> 1 role)
            .with_one::<(i64,)>((1,))
            .with_optional::<MembershipWithRoles>(Some(MembershipWithRoles {
                id: DbId::from(mem_id),
                membership: membership_row(mem_id, ws_id),
                roles: vec![JoinedRoleOnMembership {
                    id: DbId::from(role_id),
                    workspace_id: ws_id,
                    name: "WorkspaceViewer".to_string(),
                    description: None,
                    tags: vec![],
                    meta: RoleMeta::default(),
                    created_by: DbId::default(),
                    created_at: time::OffsetDateTime::UNIX_EPOCH,
                    updated_by: None,
                    updated_at: None,
                }],
            }))
            // resolve_for_membership -> membership.get_many_to_many_policies
            //   count_many guard + joined row (direct deny policy)
            .with_one::<(i64,)>((1,))
            .with_optional::<MembershipWithPolicies>(Some(MembershipWithPolicies {
                id: DbId::from(mem_id),
                membership: membership_row(mem_id, ws_id),
                policies: vec![joined_policy_on_membership(
                    ws_id,
                    "deny-update",
                    PolicyEffect::Deny,
                    None,
                )],
            }))
            // resolve_for_membership -> role.get_many_to_many_policies
            //   count_many guard + joined row (role allow policy)
            .with_one::<(i64,)>((1,))
            .with_optional::<RoleWithPolicies>(Some(RoleWithPolicies {
                id: DbId::from(role_id),
                role: RoleRow {
                    id: DbId::from(role_id),
                    workspace_id: ws_id,
                    name: "WorkspaceViewer".to_string(),
                    description: None,
                    tags: vec![],
                    meta: RoleMeta::default(),
                    audit: AuditFields::default(),
                },
                policies: vec![joined_policy_on_role(
                    ws_id,
                    "self-profile-update",
                    PolicyEffect::Allow,
                    Some("profile.account.id === user.id"),
                )],
            }));
        let svc = mock_svc(dbx);

        let set = svc.resolve_for_membership(mem_id).await?;

        assert_eq!(
            set.get("membership:update", "self", None),
            Some(PolicyEffect::Deny),
            "direct membership deny must win over any allow"
        );
        assert_eq!(
            set.get(
                "profile:update",
                "self",
                Some("profile.account.id === user.id")
            ),
            Some(PolicyEffect::Allow),
            "role-derived allow must be present in the union"
        );
        assert_eq!(
            set.get("profile:update", "self", None),
            None,
            "constraint is part of the lookup key"
        );
        assert_eq!(set.get("unknown:action", "self", None), None);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_for_membership_empty_set() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // membership.get_many_to_many -> count guard (0 roles) + row
            .with_one::<(i64,)>((0,))
            .with_optional::<MembershipWithRoles>(Some(MembershipWithRoles {
                id: DbId::from(mem_id),
                membership: membership_row(mem_id, ws_id),
                roles: vec![],
            }))
            // membership.get_many_to_many_policies -> count guard (0 policies) + row
            .with_one::<(i64,)>((0,))
            .with_optional::<MembershipWithPolicies>(Some(MembershipWithPolicies {
                id: DbId::from(mem_id),
                membership: membership_row(mem_id, ws_id),
                policies: vec![],
            }));
        let svc = mock_svc(dbx);

        let set = svc.resolve_for_membership(mem_id).await?;

        assert!(set.is_empty());
        assert_eq!(set.get("membership:update", "self", None), None);

        Ok(())
    }
}
