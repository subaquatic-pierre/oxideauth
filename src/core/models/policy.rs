use std::collections::HashMap;

use modql::filter::OpValString;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    core::{
        error::{CoreError, CoreResult},
        models::{
            audit::CoreAuditFields,
            list::{RequestFilterParams, RequestListOptions},
        },
        traits::{filter::OpValWorkspaceId, list::RequestListParams},
    },
    store::entities::{
        audit::AuditMeta,
        policy::{
            PolicyFilter as StorePolicyFilter, PolicyForCreate, PolicyForUpdate,
            PolicyMeta as StorePolicyMeta, PolicyRow,
        },
    },
};

pub use crate::store::entities::policy::PolicyEffect;

pub type PolicyMeta = StorePolicyMeta;
pub type PolicyFilter = StorePolicyFilter;

// ============================================================================
// PolicyDocument
// ============================================================================

/// The canonical policy document (JSON or YAML equivalent serialization).
///
/// Per `contracts/policy-document.md`: only `effect`, `actions`, and `resource`
/// are required; every other field is optional.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct PolicyDocument {
    /// Optional human label; unique per workspace when present.
    #[serde(default)]
    pub name: Option<String>,
    /// Required: `allow` | `deny`.
    pub effect: PolicyEffect,
    /// Optional UUID; defaults to the attachment target when omitted.
    #[serde(default)]
    pub principal_id: Option<Uuid>,
    /// Required, non-empty; `resource:action` strings; `*` allowed.
    pub actions: Vec<String>,
    /// Required: `self` | `<uuid>` | `*`.
    pub resource: String,
    /// Optional DSL expression (see [`parse_constraint`]).
    #[serde(default)]
    pub constraint: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub meta: serde_json::Value,
}

impl Default for PolicyDocument {
    fn default() -> Self {
        Self {
            name: None,
            effect: PolicyEffect::Allow,
            principal_id: None,
            actions: Vec::new(),
            resource: "self".to_string(),
            constraint: None,
            description: None,
            tags: Vec::new(),
            meta: serde_json::json!({}),
        }
    }
}

// ============================================================================
// Default "self" policy documents
// ============================================================================

/// Default "self" policy document: a member may update their own membership
/// (e.g. leave the workspace). Attached to the default `WorkspaceViewer` role
/// at workspace creation (US3).
pub fn default_self_membership_policy() -> PolicyDocument {
    PolicyDocument {
        name: Some("self-membership-update".to_string()),
        effect: PolicyEffect::Allow,
        actions: vec!["membership:update".to_string()],
        resource: "self".to_string(),
        constraint: Some("membership.account.id === user.id".to_string()),
        tags: vec!["system".to_string()],
        ..PolicyDocument::default()
    }
}

/// Default "self" policy document: a member may update their own profile.
///
/// The `profile` entity is future work; this constant still defines the policy
/// so the seeding path and the `PolicySet` runtime both agree on the action.
pub fn default_self_profile_policy() -> PolicyDocument {
    PolicyDocument {
        name: Some("self-profile-update".to_string()),
        effect: PolicyEffect::Allow,
        actions: vec!["profile:update".to_string()],
        resource: "self".to_string(),
        constraint: Some("profile.account.id === user.id".to_string()),
        tags: vec!["system".to_string()],
        ..PolicyDocument::default()
    }
}

// ============================================================================
// Constraint DSL
// ============================================================================

/// The comparison operator of a constraint expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comparator {
    /// `===` — equality.
    Equals,
    /// `!==` — inequality.
    NotEquals,
}

/// The right-hand side of a constraint expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintOperand {
    /// Another attribute path (e.g. `user.id`), resolved against the request context.
    AttributePath(String),
    /// A literal value.
    Literal(ConstraintLiteral),
}

/// A literal operand value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintLiteral {
    /// A quoted string, e.g. `"active"`.
    String(String),
    /// An integer, e.g. `42`.
    Integer(i64),
    /// A UUID literal.
    Uuid(Uuid),
    /// The reserved literal `self`.
    Self_,
    /// The wildcard literal `*`.
    Wildcard,
}

/// A parsed constraint: `attribute_path comparator operand`.
///
/// Grammar (from `contracts/policy-document.md`):
///
/// ```text
/// constraint       := attribute_path comparator operand
/// comparator       := "===" | "!=="
/// operand          := attribute_path | literal
/// attribute_path   := ident ("." ident)*
/// ident            := [A-Za-z_][A-Za-z0-9_]*
/// literal          := quoted-string | integer | uuid | "self" | "*"
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyConstraint {
    /// Left-hand attribute path (resolved against the target resource).
    pub attribute_path: String,
    pub comparator: Comparator,
    /// Right-hand operand (attribute path or literal).
    pub operand: ConstraintOperand,
}

/// Parses a constraint expression against the DSL grammar.
///
/// Returns a descriptive error string on failure.
pub fn parse_constraint(input: &str) -> Result<PolicyConstraint, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("constraint must not be empty".to_string());
    }

    // Locate the earliest comparator (`===` or `!==`).
    let eq_pos = input.find("===");
    let neq_pos = input.find("!==");
    let (pos, comparator) = match (eq_pos, neq_pos) {
        (Some(e), Some(n)) => {
            if e < n {
                (e, Comparator::Equals)
            } else {
                (n, Comparator::NotEquals)
            }
        }
        (Some(e), None) => (e, Comparator::Equals),
        (None, Some(n)) => (n, Comparator::NotEquals),
        (None, None) => {
            return Err(
                "constraint must contain a comparator `===` or `!==`".to_string()
            );
        }
    };

    let left = input[..pos].trim();
    let right = input[pos + 3..].trim();

    if left.is_empty() {
        return Err("constraint: left attribute path is empty".to_string());
    }

    let attribute_path =
        parse_attribute_path(left).map_err(|e| format!("constraint: left operand: {e}"))?;
    let operand =
        parse_operand(right).map_err(|e| format!("constraint: right operand: {e}"))?;

    Ok(PolicyConstraint {
        attribute_path,
        comparator,
        operand,
    })
}

/// Validates that `s` is a valid `attribute_path` (`ident ("." ident)*`) and
/// returns it unchanged.
fn parse_attribute_path(s: &str) -> Result<String, String> {
    if s.is_empty() {
        return Err("attribute path is empty".to_string());
    }
    for part in s.split('.') {
        parse_ident(part)?;
    }
    Ok(s.to_string())
}

/// Validates that `s` is a single valid `ident` (`[A-Za-z_][A-Za-z0-9_]*`).
fn parse_ident(s: &str) -> Result<(), String> {
    let mut chars = s.chars();
    let first = chars.next().ok_or_else(|| {
        format!("attribute path contains an empty segment in `{s}`")
    })?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(format!(
            "invalid identifier `{s}`: must start with a letter or underscore"
        ));
    }
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!(
                "invalid identifier `{s}`: only letters, digits, and underscores are allowed"
            ));
        }
    }
    Ok(())
}

/// Parses a right-hand operand: an attribute path or a literal.
fn parse_operand(s: &str) -> Result<ConstraintOperand, String> {
    if s.is_empty() {
        return Err("operand is empty".to_string());
    }

    // Quoted string.
    if s.starts_with('"') {
        if s.len() < 2 || !s.ends_with('"') {
            return Err(format!("unterminated quoted string in `{s}`"));
        }
        let inner = &s[1..s.len() - 1];
        return Ok(ConstraintOperand::Literal(ConstraintLiteral::String(
            inner.to_string(),
        )));
    }

    // Reserved literals.
    if s == "self" {
        return Ok(ConstraintOperand::Literal(ConstraintLiteral::Self_));
    }
    if s == "*" {
        return Ok(ConstraintOperand::Literal(ConstraintLiteral::Wildcard));
    }

    // Integer.
    if let Ok(i) = s.parse::<i64>() {
        return Ok(ConstraintOperand::Literal(ConstraintLiteral::Integer(i)));
    }

    // UUID.
    if let Ok(u) = Uuid::parse_str(s) {
        return Ok(ConstraintOperand::Literal(ConstraintLiteral::Uuid(u)));
    }

    // Otherwise: an attribute path.
    parse_attribute_path(s).map(ConstraintOperand::AttributePath)
}

// ============================================================================
// Runtime key compilation
// ============================================================================

/// Compiles a policy into its canonical runtime key.
///
/// `effect|sort(actions).join(",")|resource|(constraint or "")`
///
/// This key is unique per workspace and is the map key of a member's
/// `PolicySet`, enabling O(1) lookup (see `research.md` decision 2).
pub fn runtime_key(
    effect: PolicyEffect,
    actions: &[String],
    resource: &str,
    constraint: Option<&str>,
) -> String {
    let mut sorted: Vec<&String> = actions.iter().collect();
    sorted.sort();
    let actions_str = sorted
        .into_iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let constraint_str = constraint.unwrap_or("");
    format!("{}|{}|{}|{}", effect, actions_str, resource, constraint_str)
}

// ============================================================================
// PolicySet
// ============================================================================

/// Compiles the per-action lookup key for a [`PolicySet`].
///
/// `action|resource|(constraint or "")` — a single (action, resource,
/// constraint) triple resolves in O(1) against the compiled set.
pub fn policy_lookup_key(action: &str, resource: &str, constraint: Option<&str>) -> String {
    format!("{action}|{resource}|{}", constraint.unwrap_or(""))
}

/// A compiled, per-action policy lookup table for a principal.
///
/// Built from the effective policies of a membership (roles + direct
/// attachments). Lookup is O(1) per `action|resource|constraint` triple; the
/// `Deny` effect wins on collision (US4).
///
/// `Serialize`/`Deserialize` support the `oxauth:policy:{mem_id}` cache entity
/// (US6): the compiled set is persisted as-is and hydrated back on a cache hit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicySet(HashMap<String, PolicyEffect>);

impl PolicySet {
    /// Compiles a list of policies into the lookup table.
    ///
    /// Each policy expands into one entry **per action**. On collision
    /// (`deny-overrides-allow`) the `Deny` effect is kept: an existing `Deny`
    /// entry is never overwritten by an `Allow`, while an `Allow` entry may be
    /// upgraded to `Deny`.
    pub fn from_policies(policies: Vec<Policy>) -> Self {
        let mut map = HashMap::new();
        for policy in policies {
            for action in policy.actions {
                let key =
                    policy_lookup_key(&action, &policy.resource, policy.constraint.as_deref());
                // Deny wins on collision.
                if map.get(&key) == Some(&PolicyEffect::Deny) {
                    continue;
                }
                map.insert(key, policy.effect.clone());
            }
        }
        Self(map)
    }

    /// Resolves the effect for an `action` on `resource` under `constraint`.
    ///
    /// `constraint` defaults to the empty string when `None`, matching
    /// [`policy_lookup_key`].
    pub fn get(
        &self,
        action: &str,
        resource: &str,
        constraint: Option<&str>,
    ) -> Option<PolicyEffect> {
        self.0
            .get(&policy_lookup_key(action, resource, constraint))
            .cloned()
    }

    /// Number of distinct `action|resource|constraint` entries.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the set contains no entries.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns `true` when any `Allow` entry in the set is keyed with
    /// `|{constraint}` as the trailing segment and **no** `Deny` entry
    /// overrides it (deny precedence applies).
    ///
    /// Unlike [`PolicySet::get`] — which is an O(1) exact-key lookup by
    /// `action|resource|constraint` — this helper scans the whole set and is
    /// agnostic to the action/resource prefixes. Use it only for low-frequency,
    /// constraint-centric checks (e.g. the client-validation path in
    /// `ClientService`). The 1,000 req/s service hot path stays O(1) via
    /// exact-key lookup.
    pub fn allows_constraint(&self, constraint: &str) -> bool {
        let suffix = format!("|{constraint}");
        let mut allowed = false;
        for (key, effect) in self.0.iter() {
            if !key.ends_with(&suffix) {
                continue;
            }
            match effect {
                // An explicit deny for the constraint overrides any allow.
                PolicyEffect::Deny => return false,
                PolicyEffect::Allow => allowed = true,
            }
        }
        allowed
    }
}

// ============================================================================
// Policy (core model)
// ============================================================================

/// The workspace-scoped authorization rule surfaced by the service layer.
///
/// Mirrors `Role`/`Permission` (id, workspace_id, tags, meta, audit) plus the
/// AWS-like policy body (`effect`, `principal_id`, `actions`, `resource`,
/// `constraint`) per `data-model.md`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Policy {
    pub id: Uuid,
    pub workspace_id: Uuid,

    /// Human label; unique per workspace when present.
    pub name: Option<String>,
    /// `allow` | `deny`.
    pub effect: PolicyEffect,
    /// Optional UUID; defaults to the attachment target when omitted.
    pub principal_id: Option<Uuid>,
    /// `resource:action` strings; `*` allowed. Non-empty.
    pub actions: Vec<String>,
    /// `self` | `<uuid>` | `*`.
    pub resource: String,
    /// Optional DSL expression.
    pub constraint: Option<String>,
    pub description: Option<String>,

    pub tags: Vec<String>,
    pub meta: PolicyMeta,

    pub audit: CoreAuditFields,
}

impl From<PolicyRow> for Policy {
    fn from(row: PolicyRow) -> Self {
        Self {
            id: row.id.into(),
            workspace_id: row.workspace_id,
            name: row.name,
            effect: row.effect,
            principal_id: row.principal_id,
            actions: row.actions,
            resource: row.resource,
            constraint: row.constraint_expr,
            description: row.description,
            tags: row.tags,
            meta: row.meta,
            // The `policy` table carries only `created_at`/`updated_at` audit
            // columns (no `created_by`/`updated_by`), so the identity fields
            // default to nil / None.
            audit: CoreAuditFields {
                created_by: Uuid::nil(),
                created_at: row.created_at,
                updated_by: None,
                updated_at: row.updated_at,
                meta: AuditMeta::default(),
            },
        }
    }
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace_id: Uuid::nil(),
            name: None,
            effect: PolicyEffect::Allow,
            principal_id: None,
            actions: vec!["membership:update".to_string()],
            resource: "self".to_string(),
            constraint: None,
            description: None,
            tags: vec![],
            meta: PolicyMeta {
                schema_version: "1".to_string(),
            },
            audit: CoreAuditFields::default(),
        }
    }
}

/// Params for creating a new `policy`.
#[derive(Debug, Deserialize)]
pub struct PolicyCreateParams {
    pub workspace_id: Uuid,
    pub name: Option<String>,
    pub effect: PolicyEffect,
    pub principal_id: Option<Uuid>,
    pub actions: Vec<String>,
    pub resource: String,
    pub constraint: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub meta: PolicyMeta,
}

impl PolicyCreateParams {
    /// Builds create params from a [`PolicyDocument`] for the given workspace.
    ///
    /// The document mirrors the AWS-like request body; validation happens in the
    /// service layer (`PolicyService::create`).
    pub fn from_document(workspace_id: Uuid, document: PolicyDocument) -> Self {
        Self {
            workspace_id,
            name: document.name,
            effect: document.effect,
            principal_id: document.principal_id,
            actions: document.actions,
            resource: document.resource,
            constraint: document.constraint,
            description: document.description,
            tags: document.tags,
            meta: PolicyMeta {
                schema_version: document
                    .meta
                    .get("schema_version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("1")
                    .to_string(),
            },
        }
    }
}

impl From<PolicyCreateParams> for PolicyForCreate {
    fn from(params: PolicyCreateParams) -> Self {
        Self {
            workspace_id: params.workspace_id,
            name: params.name,
            effect: params.effect,
            principal_id: params.principal_id,
            actions: params.actions,
            resource: params.resource,
            constraint_expr: params.constraint,
            description: params.description,
            tags: params.tags,
            meta: params.meta,
        }
    }
}

/// Params for updating an existing `policy` (all fields optional; `id` required).
#[derive(Debug, Deserialize)]
pub struct PolicyUpdateParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: Option<String>,
    pub effect: Option<PolicyEffect>,
    pub principal_id: Option<Uuid>,
    pub actions: Option<Vec<String>>,
    pub resource: Option<String>,
    pub constraint: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<PolicyMeta>,
}

impl From<PolicyUpdateParams> for PolicyForUpdate {
    fn from(params: PolicyUpdateParams) -> Self {
        Self {
            name: params.name,
            effect: params.effect,
            principal_id: params.principal_id,
            actions: params.actions,
            resource: params.resource,
            constraint_expr: params.constraint,
            description: params.description,
            tags: params.tags,
            meta: params.meta,
        }
    }
}

/// Params for describing a single `policy` by id.
#[derive(Debug, Deserialize)]
pub struct PolicyDescribeParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

pub struct PolicyDeleteParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

pub struct PolicyListParams {
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<PolicyFilter>>,
    pub options: Option<RequestListOptions>,
}

impl RequestListParams<PolicyFilter> for PolicyListParams {
    fn filter(&self) -> Option<RequestFilterParams<PolicyFilter>> {
        self.filter.clone()
    }

    fn options(&self) -> Option<RequestListOptions> {
        self.options.clone()
    }
}

impl OpValWorkspaceId for PolicyFilter {
    fn get_workspace_id_opval(&self) -> Option<&OpValString> {
        self.workspace_id
            .as_ref()
            .and_then(|op_vals| op_vals.0.first())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_constraint: valid inputs ---

    #[test]
    fn test_parse_constraint_equality_paths() {
        let c = parse_constraint("profile.account.id === user.id").unwrap();
        assert_eq!(c.attribute_path, "profile.account.id");
        assert_eq!(c.comparator, Comparator::Equals);
        assert_eq!(
            c.operand,
            ConstraintOperand::AttributePath("user.id".to_string())
        );
    }

    #[test]
    fn test_parse_constraint_inequality() {
        let c = parse_constraint("project.owner.id !== user.id").unwrap();
        assert_eq!(c.attribute_path, "project.owner.id");
        assert_eq!(c.comparator, Comparator::NotEquals);
    }

    #[test]
    fn test_parse_constraint_single_ident() {
        let c = parse_constraint("status === \"active\"").unwrap();
        assert_eq!(c.attribute_path, "status");
        assert_eq!(
            c.operand,
            ConstraintOperand::Literal(ConstraintLiteral::String(
                "active".to_string()
            ))
        );
    }

    #[test]
    fn test_parse_constraint_integer_literal() {
        let c = parse_constraint("port === 8080").unwrap();
        assert_eq!(
            c.operand,
            ConstraintOperand::Literal(ConstraintLiteral::Integer(8080))
        );
    }

    #[test]
    fn test_parse_constraint_uuid_literal() {
        let u = "123e4567-e89b-12d3-a456-426614174000";
        let c = parse_constraint(&format!("account.id === {u}")).unwrap();
        assert_eq!(
            c.operand,
            ConstraintOperand::Literal(ConstraintLiteral::Uuid(
                Uuid::parse_str(u).unwrap()
            ))
        );
    }

    #[test]
    fn test_parse_constraint_self_and_wildcard() {
        let c = parse_constraint("resource.id === self").unwrap();
        assert_eq!(
            c.operand,
            ConstraintOperand::Literal(ConstraintLiteral::Self_)
        );

        let c = parse_constraint("resource.scope !== *").unwrap();
        assert_eq!(
            c.operand,
            ConstraintOperand::Literal(ConstraintLiteral::Wildcard)
        );
    }

    #[test]
    fn test_parse_constraint_underscore_idents() {
        let c = parse_constraint("_private.field_2 === user.id").unwrap();
        assert_eq!(c.attribute_path, "_private.field_2");
        assert_eq!(
            c.operand,
            ConstraintOperand::AttributePath("user.id".to_string())
        );
    }

    #[test]
    fn test_parse_constraint_whitespace_tolerant() {
        let c = parse_constraint("  blog.author.id   ===   user.id  ").unwrap();
        assert_eq!(c.attribute_path, "blog.author.id");
        assert_eq!(
            c.operand,
            ConstraintOperand::AttributePath("user.id".to_string())
        );
    }

    // --- parse_constraint: invalid inputs ---

    #[test]
    fn test_parse_constraint_invalid_empty() {
        assert!(parse_constraint("").is_err());
        assert!(parse_constraint("   ").is_err());
    }

    #[test]
    fn test_parse_constraint_invalid_missing_comparator() {
        assert!(parse_constraint("profile.account.id user.id").is_err());
        assert!(parse_constraint("a = b").is_err());
        assert!(parse_constraint("a == b").is_err());
    }

    #[test]
    fn test_parse_constraint_invalid_empty_side() {
        assert!(parse_constraint("=== user.id").is_err());
        assert!(parse_constraint("profile.id === ").is_err());
    }

    #[test]
    fn test_parse_constraint_invalid_ident() {
        assert!(parse_constraint("1abc === user.id").is_err());
        assert!(parse_constraint("profile.account-id === user.id").is_err());
        assert!(parse_constraint("a..b === c").is_err());
    }

    #[test]
    fn test_parse_constraint_invalid_operand() {
        assert!(parse_constraint("a === \"unterminated").is_err());
        assert!(parse_constraint("a === b c").is_err());
    }

    // --- runtime_key ---

    #[test]
    fn test_runtime_key_sorts_actions() {
        let k1 = runtime_key(
            PolicyEffect::Allow,
            &["membership:update".to_string(), "profile:update".to_string()],
            "self",
            Some("profile.account.id === user.id"),
        );
        let k2 = runtime_key(
            PolicyEffect::Allow,
            &["profile:update".to_string(), "membership:update".to_string()],
            "self",
            Some("profile.account.id === user.id"),
        );
        assert_eq!(k1, k2, "runtime_key must be independent of action order");
        assert_eq!(
            k1,
            "allow|membership:update,profile:update|self|profile.account.id === user.id"
        );
    }

    #[test]
    fn test_runtime_key_no_constraint() {
        let k = runtime_key(
            PolicyEffect::Deny,
            &["membership:delete".to_string()],
            "*",
            None,
        );
        assert_eq!(k, "deny|membership:delete|*|");
    }

    #[test]
    fn test_runtime_key_deterministic() {
        let actions: Vec<String> = vec![
            "z:1".to_string(),
            "a:1".to_string(),
            "m:1".to_string(),
            "b:1".to_string(),
        ];
        let expected = "allow|a:1,b:1,m:1,z:1|self|";
        for _ in 0..10 {
            assert_eq!(
                runtime_key(PolicyEffect::Allow, &actions, "self", None),
                expected
            );
        }
    }

    #[test]
    fn test_runtime_key_effect_differentiates() {
        let allow_key = runtime_key(PolicyEffect::Allow, &["a:1".to_string()], "self", None);
        let deny_key = runtime_key(PolicyEffect::Deny, &["a:1".to_string()], "self", None);
        assert_ne!(allow_key, deny_key);
    }

    // --- Policy model conversions ---

    fn make_policy_row() -> PolicyRow {
        PolicyRow {
            id: Uuid::new_v4().into(),
            workspace_id: Uuid::new_v4(),
            name: Some("self-update".to_string()),
            effect: PolicyEffect::Allow,
            principal_id: None,
            actions: vec!["membership:update".to_string(), "profile:update".to_string()],
            resource: "self".to_string(),
            constraint_expr: Some("profile.account.id === user.id".to_string()),
            description: Some("Members may update their own profile".to_string()),
            tags: vec!["system".to_string()],
            meta: PolicyMeta {
                schema_version: "1".to_string(),
            },
            created_at: time::OffsetDateTime::UNIX_EPOCH,
            updated_at: None,
        }
    }

    #[test]
    fn test_policy_from_row() {
        let row = make_policy_row();
        let policy = Policy::from(row);

        assert_eq!(policy.name.as_deref(), Some("self-update"));
        assert_eq!(policy.effect, PolicyEffect::Allow);
        assert_eq!(policy.actions, vec!["membership:update".to_string(), "profile:update".to_string()]);
        assert_eq!(policy.resource, "self");
        assert_eq!(policy.constraint.as_deref(), Some("profile.account.id === user.id"));
        assert_eq!(policy.description.as_deref(), Some("Members may update their own profile"));
        assert_eq!(policy.tags, vec!["system".to_string()]);
        assert_eq!(policy.meta.schema_version, "1");
        assert_eq!(policy.audit.created_at, time::OffsetDateTime::UNIX_EPOCH);
        // The policy table carries no created_by/updated_by columns.
        assert_eq!(policy.audit.created_by, Uuid::nil());
        assert!(policy.audit.updated_by.is_none());
    }

    #[test]
    fn test_policy_default() {
        let policy = Policy::default();
        assert_eq!(policy.workspace_id, Uuid::nil());
        assert_eq!(policy.effect, PolicyEffect::Allow);
        assert_eq!(policy.actions, vec!["membership:update".to_string()]);
        assert_eq!(policy.resource, "self");
        assert!(policy.name.is_none());
        assert!(policy.principal_id.is_none());
        assert!(policy.constraint.is_none());
        assert_eq!(policy.meta.schema_version, "1");
    }

    #[test]
    fn test_policy_create_params_into_store() {
        let ws_id = Uuid::new_v4();
        let params = PolicyCreateParams {
            workspace_id: ws_id,
            name: Some("self-update".to_string()),
            effect: PolicyEffect::Deny,
            principal_id: None,
            actions: vec!["membership:delete".to_string()],
            resource: "*".to_string(),
            constraint: Some("project.owner.id !== user.id".to_string()),
            description: Some("desc".to_string()),
            tags: vec!["t".to_string()],
            meta: PolicyMeta {
                schema_version: "1".to_string(),
            },
        };

        let store: PolicyForCreate = params.into();
        assert_eq!(store.workspace_id, ws_id);
        assert_eq!(store.name.as_deref(), Some("self-update"));
        assert_eq!(store.effect, PolicyEffect::Deny);
        assert_eq!(store.actions, vec!["membership:delete".to_string()]);
        assert_eq!(store.resource, "*");
        assert_eq!(store.constraint_expr.as_deref(), Some("project.owner.id !== user.id"));
        assert_eq!(store.tags, vec!["t".to_string()]);
    }

    #[test]
    fn test_policy_create_params_from_document() {
        let ws_id = Uuid::new_v4();
        let document = PolicyDocument {
            name: Some("self-update".to_string()),
            effect: PolicyEffect::Allow,
            principal_id: Some(Uuid::new_v4()),
            actions: vec!["membership:update".to_string()],
            resource: "self".to_string(),
            constraint: Some("membership.account.id === user.id".to_string()),
            description: None,
            tags: vec![],
            meta: serde_json::json!({ "schema_version": "2" }),
        };

        let params = PolicyCreateParams::from_document(ws_id, document);
        assert_eq!(params.workspace_id, ws_id);
        assert_eq!(params.name.as_deref(), Some("self-update"));
        assert_eq!(params.actions, vec!["membership:update".to_string()]);
        assert_eq!(params.resource, "self");
        assert_eq!(params.meta.schema_version, "2");
    }

    #[test]
    fn test_policy_update_params_into_store() {
        let params = PolicyUpdateParams {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            name: Some("N".to_string()),
            effect: Some(PolicyEffect::Deny),
            principal_id: Some(Uuid::new_v4()),
            actions: Some(vec!["membership:update".to_string()]),
            resource: Some("self".to_string()),
            constraint: Some("membership.account.id === user.id".to_string()),
            description: Some("d".to_string()),
            tags: Some(vec!["t".to_string()]),
            meta: Some(PolicyMeta {
                schema_version: "2".to_string(),
            }),
        };

        let store: PolicyForUpdate = params.into();
        assert_eq!(store.name.as_deref(), Some("N"));
        assert_eq!(store.effect, Some(PolicyEffect::Deny));
        assert_eq!(store.actions, Some(vec!["membership:update".to_string()]));
        assert_eq!(store.resource.as_deref(), Some("self"));
        assert_eq!(store.meta.unwrap().schema_version, "2");
    }

    #[test]
    fn test_policy_list_params_accessors() {
        let params = PolicyListParams {
            workspace_id: Uuid::new_v4(),
            filter: None,
            options: None,
        };
        assert!(params.filter().is_none());
        assert!(params.options().is_none());
    }

    #[test]
    fn test_policy_filter_workspace_id_opval() {
        use crate::core::traits::filter::OpValIsString;

        let filter = PolicyFilter::default();
        assert!(filter.get_workspace_id_opval().is_none());

        let ws_id = Uuid::new_v4();
        let filter: PolicyFilter = serde_json::from_value(serde_json::json!({
            "workspace_id": ws_id.to_string()
        }))
        .expect("filter should deserialize");

        let opval = filter.get_workspace_id_opval().expect("ws present");
        assert_eq!(opval.as_eq_string(), Some(ws_id.to_string().as_str()));
    }

    // --- default "self" policy documents (US3) ---

    #[test]
    fn test_default_self_membership_policy() {
        let doc = default_self_membership_policy();
        assert_eq!(doc.name.as_deref(), Some("self-membership-update"));
        assert_eq!(doc.effect, PolicyEffect::Allow);
        assert_eq!(doc.actions, vec!["membership:update".to_string()]);
        assert_eq!(doc.resource, "self");
        assert_eq!(
            doc.constraint.as_deref(),
            Some("membership.account.id === user.id")
        );
        assert_eq!(doc.tags, vec!["system".to_string()]);
        assert!(doc.principal_id.is_none());
        assert!(doc.description.is_none());
    }

    #[test]
    fn test_default_self_profile_policy() {
        let doc = default_self_profile_policy();
        assert_eq!(doc.name.as_deref(), Some("self-profile-update"));
        assert_eq!(doc.effect, PolicyEffect::Allow);
        assert_eq!(doc.actions, vec!["profile:update".to_string()]);
        assert_eq!(doc.resource, "self");
        assert_eq!(
            doc.constraint.as_deref(),
            Some("profile.account.id === user.id")
        );
        assert_eq!(doc.tags, vec!["system".to_string()]);
    }

    #[test]
    fn test_default_self_policy_documents_roundtrip_to_create_params() {
        for doc in [default_self_membership_policy(), default_self_profile_policy()] {
            let ws_id = Uuid::new_v4();
            let params = PolicyCreateParams::from_document(ws_id, doc);
            assert_eq!(params.workspace_id, ws_id);
            assert!(!params.actions.is_empty());
            assert_eq!(params.resource, "self");
            assert!(params.constraint.is_some());
        }
    }

    // --- PolicySet (US4) ---

    fn policy(
        id: Uuid,
        effect: PolicyEffect,
        actions: Vec<&str>,
        resource: &str,
        constraint: Option<&str>,
    ) -> Policy {
        Policy {
            id,
            effect,
            actions: actions.into_iter().map(|s| s.to_string()).collect(),
            resource: resource.to_string(),
            constraint: constraint.map(|s| s.to_string()),
            ..Policy::default()
        }
    }

    #[test]
    fn test_policy_set_multi_action_expansion() {
        let mut p = Policy::default();
        p.id = Uuid::new_v4();
        p.actions = vec![
            "membership:update".to_string(),
            "profile:update".to_string(),
        ];
        p.resource = "self".to_string();
        p.constraint = Some("membership.account.id === user.id".to_string());

        let set = PolicySet::from_policies(vec![p]);

        // Each action resolves independently to the same key's effect.
        assert_eq!(
            set.get("membership:update", "self", Some("membership.account.id === user.id")),
            Some(PolicyEffect::Allow)
        );
        assert_eq!(
            set.get("profile:update", "self", Some("membership.account.id === user.id")),
            Some(PolicyEffect::Allow)
        );
        assert_eq!(set.len(), 2, "one entry per distinct action");
    }

    #[test]
    fn test_policy_set_deny_overrides_allow() {
        let allow = policy(
            Uuid::new_v4(),
            PolicyEffect::Allow,
            vec!["membership:update"],
            "self",
            None,
        );
        let deny = policy(
            Uuid::new_v4(),
            PolicyEffect::Deny,
            vec!["membership:update"],
            "self",
            None,
        );

        // Deny applied after allow: deny wins.
        let set = PolicySet::from_policies(vec![allow.clone(), deny.clone()]);
        assert_eq!(
            set.get("membership:update", "self", None),
            Some(PolicyEffect::Deny)
        );

        // Allow applied after deny: deny still wins.
        let set = PolicySet::from_policies(vec![deny, allow]);
        assert_eq!(
            set.get("membership:update", "self", None),
            Some(PolicyEffect::Deny)
        );
    }

    #[test]
    fn test_policy_set_default_miss() {
        let set = PolicySet::from_policies(vec![policy(
            Uuid::new_v4(),
            PolicyEffect::Allow,
            vec!["membership:update"],
            "self",
            None,
        )]);

        // Missing action.
        assert_eq!(set.get("membership:delete", "self", None), None);
        // Missing resource.
        assert_eq!(set.get("membership:update", "*", None), None);
        // Different constraint (constraint part of the key).
        assert_eq!(
            set.get("membership:update", "self", Some("a !== b")),
            None
        );
        // Empty set.
        let empty = PolicySet::from_policies(vec![]);
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert_eq!(empty.get("membership:update", "self", None), None);
    }

    #[test]
    fn test_policy_set_constraint_defaults_to_empty() {
        let mut p = Policy::default();
        p.id = Uuid::new_v4();
        p.actions = vec!["membership:update".to_string()];
        p.resource = "self".to_string();
        p.constraint = None;

        let set = PolicySet::from_policies(vec![p]);
        // Lookup with None and lookup with Some("") both hit the same key.
        assert_eq!(set.get("membership:update", "self", None), Some(PolicyEffect::Allow));
        assert_eq!(set.get("membership:update", "self", Some("")), Some(PolicyEffect::Allow));
    }

    #[test]
    fn test_policy_lookup_key_shape() {
        assert_eq!(
            policy_lookup_key("membership:update", "self", Some("a === b")),
            "membership:update|self|a === b"
        );
        assert_eq!(
            policy_lookup_key("membership:update", "self", None),
            "membership:update|self|"
        );
    }

    // --- allows_constraint (US5/T038) ---

    const SELF_CONSTRAINT: &str = "membership.account.id === user.id";

    #[test]
    fn test_policy_set_allows_constraint_when_any_allow_matches() {
        let set = PolicySet::from_policies(vec![
            policy(
                Uuid::new_v4(),
                PolicyEffect::Allow,
                vec!["membership:update"],
                "self",
                Some(SELF_CONSTRAINT),
            ),
            // An unrelated allow entry (different constraint) must not count.
            policy(
                Uuid::new_v4(),
                PolicyEffect::Allow,
                vec!["profile:update"],
                "self",
                Some("profile.account.id === user.id"),
            ),
        ]);

        assert!(
            set.allows_constraint(SELF_CONSTRAINT),
            "an allow entry ending with the constraint suffix must grant it"
        );
        assert!(
            set.allows_constraint("profile.account.id === user.id"),
            "a different allow entry grants its own constraint"
        );
    }

    #[test]
    fn test_policy_set_allows_constraint_deny_overrides() {
        // Deny wins on the same compiled key, even when an allow is also present.
        let deny_wins = PolicySet::from_policies(vec![
            policy(
                Uuid::new_v4(),
                PolicyEffect::Allow,
                vec!["membership:update"],
                "self",
                Some(SELF_CONSTRAINT),
            ),
            policy(
                Uuid::new_v4(),
                PolicyEffect::Deny,
                vec!["membership:update"],
                "self",
                Some(SELF_CONSTRAINT),
            ),
        ]);
        assert!(
            !deny_wins.allows_constraint(SELF_CONSTRAINT),
            "an explicit deny for the constraint overrides any allow"
        );

        // A deny for the constraint on a different action also overrides: deny
        // precedence applies to the constraint as a whole.
        let deny_elsewhere = PolicySet::from_policies(vec![
            policy(
                Uuid::new_v4(),
                PolicyEffect::Allow,
                vec!["membership:update"],
                "self",
                Some(SELF_CONSTRAINT),
            ),
            policy(
                Uuid::new_v4(),
                PolicyEffect::Deny,
                vec!["membership:delete"],
                "self",
                Some(SELF_CONSTRAINT),
            ),
        ]);
        assert!(
            !deny_elsewhere.allows_constraint(SELF_CONSTRAINT),
            "any deny entry for the constraint overrides all allows"
        );
    }

    #[test]
    fn test_policy_set_allows_constraint_no_match() {
        let set = PolicySet::from_policies(vec![
            // Constrained allow on a different constraint.
            policy(
                Uuid::new_v4(),
                PolicyEffect::Allow,
                vec!["membership:update"],
                "self",
                Some("profile.account.id === user.id"),
            ),
            // Unconstrained allow (key ends with `|`) must not match either.
            policy(
                Uuid::new_v4(),
                PolicyEffect::Allow,
                vec!["membership:update"],
                "self",
                None,
            ),
        ]);

        assert!(
            !set.allows_constraint(SELF_CONSTRAINT),
            "no entry with the constraint suffix must not grant it"
        );
        assert!(
            !PolicySet::default().allows_constraint(SELF_CONSTRAINT),
            "an empty set never grants a constraint"
        );
    }
}
