use std::{
    collections::{HashMap, HashSet},
    fmt::{Display, write},
};

use modql::filter::OpValString;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    core::{
        error::{CoreError, CoreResult},
        models::{
            audit::CoreAuditFields,
            list::{RequestFilterParams, RequestListOptions},
            workspace::Workspace,
        },
        traits::{filter::OpValWorkspaceId, list::RequestListParams, params::ValidateParams},
    },
    store::entities::{
        audit::AuditMeta,
        permission::{
            PermissionFilter as StorePermissionFilter, PermissionForCreate, PermissionForUpdate,
            PermissionMeta as StorePermissionMeta, PermissionRow,
        },
        role::JoinedPermissionOnRole,
    },
};

pub type PermissionMeta = StorePermissionMeta;
pub type PermissionFilter = StorePermissionFilter;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Permission {
    pub id: Uuid,
    pub workspace_id: Uuid,

    pub name: String,
    pub description: Option<String>,

    pub tags: Vec<String>,
    pub meta: PermissionMeta,

    pub audit: CoreAuditFields,
}

impl From<PermissionRow> for Permission {
    fn from(row: PermissionRow) -> Self {
        Self {
            id: row.id.into(),
            workspace_id: row.workspace_id,
            name: row.name,
            description: row.description,
            tags: row.tags,
            meta: row.meta,
            audit: row.audit.into(),
        }
    }
}

impl From<JoinedPermissionOnRole> for Permission {
    fn from(row: JoinedPermissionOnRole) -> Self {
        Self {
            id: row.id.into(),
            workspace_id: row.workspace_id,
            name: row.name,
            description: row.description,
            tags: row.tags,
            meta: row.meta,
            audit: CoreAuditFields {
                created_by: row.created_by.into(),
                created_at: row.created_at,
                updated_by: row.updated_by.map(|el| el.into()),
                updated_at: row.updated_at,
                meta: AuditMeta {
                    schema_version: "DO_NOT_USE".into(),
                },
            },
        }
    }
}

impl Default for Permission {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace_id: Uuid::nil(),
            name: "New Permission".to_string(),
            description: None,
            tags: vec![],
            meta: PermissionMeta {
                schema_version: "1".to_string(),
            },
            audit: CoreAuditFields::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PermissionCreateParams {
    pub workspace_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub meta: PermissionMeta,
}

impl PermissionCreateParams {
    pub fn new_system(ws_id: Uuid, name: &str, desc: Option<&str>) -> Self {
        Self {
            workspace_id: ws_id,
            name: name.to_string(),
            description: desc.map(|s| s.to_string()),
            tags: vec!["system".to_string()],
            meta: PermissionMeta::default(),
        }
    }
}

impl From<PermissionCreateParams> for PermissionForCreate {
    fn from(value: PermissionCreateParams) -> Self {
        PermissionForCreate {
            workspace_id: value.workspace_id.into(),
            name: value.name,
            description: value.description,
            tags: value.tags,
            meta: value.meta,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PermissionUpdateParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<PermissionMeta>,
}

impl From<PermissionUpdateParams> for PermissionForUpdate {
    fn from(params: PermissionUpdateParams) -> Self {
        Self {
            name: params.name,
            description: params.description,
            tags: params.tags,
            meta: params.meta,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PermissionDescribeParams {
    pub id: Option<Uuid>,
    pub workspace_id: Uuid,
}

impl ValidateParams for PermissionDescribeParams {
    fn validate(self) -> CoreResult<Self> {
        if self.id.is_none() {
            return Err(CoreError::InvalidParams(
                "Permission describe must contain id".into(),
            ));
        }

        Ok(self)
    }
}

pub struct PermissionDeleteParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

pub struct PermissionListParams {
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<PermissionFilter>>,
    pub options: Option<RequestListOptions>,
}

impl RequestListParams<PermissionFilter> for PermissionListParams {
    fn filter(&self) -> Option<RequestFilterParams<PermissionFilter>> {
        self.filter.clone()
    }

    fn options(&self) -> Option<RequestListOptions> {
        self.options.clone()
    }
}

impl OpValWorkspaceId for PermissionFilter {
    fn get_workspace_id_opval(&self) -> Option<&OpValString> {
        self.workspace_id
            .as_ref()
            .and_then(|op_vals| op_vals.0.first())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq, Hash)]
pub struct PermissionRule {
    resource: String,
    action: String,
}

impl PermissionRule {
    /// Checks if this pattern matches a required permission.
    /// `self` represents the granted pattern (e.g., from the user's roles).
    /// `required_resource` and `required_action` are the specific permissions
    /// being checked (e.g., "projects", "delete").
    pub fn matches(&self, perm: &PermissionRule) -> bool {
        PermissionRule::matches_str(&self.action, &perm.action)
            && PermissionRule::matches_str(&self.resource, &perm.resource)
    }

    pub fn perms_from_str_slice(perms: &[&str]) -> CoreResult<Vec<PermissionRule>> {
        let perms: CoreResult<Vec<PermissionRule>> = perms
            .iter()
            .map(|&el| PermissionRule::try_from(el))
            .collect();

        perms
    }

    pub fn perms_from_string_slice(perms: &[String]) -> CoreResult<Vec<PermissionRule>> {
        let mut data: Vec<PermissionRule> = vec![];

        for perm in perms {
            data.push(PermissionRule::try_from(perm)?);
        }

        Ok(data)
    }

    fn matches_str(granted: &str, required: &str) -> bool {
        if required == "*" || granted == "*" {
            return true;
        }

        if granted == required {
            return true;
        }

        return false;
    }
}

impl Display for PermissionRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.resource, self.action);
        Ok(())
    }
}

impl TryFrom<&String> for PermissionRule {
    type Error = CoreError;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl TryFrom<String> for PermissionRule {
    type Error = CoreError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl TryFrom<&str> for PermissionRule {
    type Error = CoreError;

    // Parses a string like "projects:create" into the struct.
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value == "*" {
            return Ok({
                Self {
                    resource: "*".to_string(),
                    action: "*".to_string(),
                }
            });
        }
        // Find the position of the first ':'
        if let Some(index) = value.find(':') {
            let (resource, action_with_colon) = value.split_at(index);
            // The action part includes the colon, so we slice it off.
            let action = &action_with_colon[1..];

            if resource.is_empty() || action.is_empty() {
                Err(CoreError::ParseError(
                    "PermissionRule string cannot have empty parts.".to_string(),
                ))
            } else {
                Ok(Self {
                    resource: resource.to_string(),
                    action: action.to_string(),
                })
            }
        } else {
            Err(CoreError::ParseError(
                "PermissionRule string must contain a ':' delimiter.".to_string(),
            ))
        }
    }
}

#[derive(Clone, Debug)]
pub struct PermissionEngine {
    // Key: resource name (e.g., "projects").
    // Value: Set of actions for that resource (e.g., {"*", "read"}).
    granted: HashMap<String, HashSet<String>>,
}

impl PermissionEngine {
    pub fn new(perms: Vec<PermissionRule>) -> Self {
        let mut _self = Self {
            granted: HashMap::new(),
        };
        _self.extend(perms);
        _self
    }

    pub fn from_str_slice(perms: &[&str]) -> CoreResult<PermissionEngine> {
        let perms: CoreResult<Vec<PermissionRule>> = perms
            .iter()
            .map(|&el| PermissionRule::try_from(el))
            .collect();

        let perms = perms?;

        let checker = PermissionEngine::new(perms);

        Ok(checker)
    }

    pub fn from_string_vec(perms: Vec<String>) -> CoreResult<PermissionEngine> {
        let perms: CoreResult<Vec<PermissionRule>> = perms
            .iter()
            .map(|el| PermissionRule::try_from(el.as_str()))
            .collect();

        let perms = perms?;

        let checker = PermissionEngine::new(perms);

        Ok(checker)
    }

    pub fn extend(&mut self, perms: Vec<PermissionRule>) {
        for perm in perms {
            if let Some(resource) = self.granted.get_mut(&perm.resource) {
                resource.insert(perm.action);
            } else {
                let mut actions = HashSet::new();
                actions.insert(perm.action);
                self.granted.insert(perm.resource, actions);
            }
        }
    }

    pub fn has_subset(&self, required: &[PermissionRule]) -> bool {
        required.iter().all(|needed| self.is_allowed(needed))
    }

    pub fn perms_vec(&self) -> CoreResult<Vec<PermissionRule>> {
        let mut data: HashSet<PermissionRule> = HashSet::new();

        for (resource, perms) in self.granted.iter() {
            let perms_vec: Vec<String> =
                perms.iter().map(|el| format!("{resource}:{el}")).collect();

            let perms_vec = PermissionRule::perms_from_string_slice(&perms_vec)?;

            data.extend(perms_vec.into_iter());
        }

        Ok(Vec::from_iter(data.into_iter()))
    }

    pub fn is_allowed(&self, required: &PermissionRule) -> bool {
        // Check for a global resource wildcard first, e.g., "*:delete"
        if let Some(actions) = self.granted.get("*") {
            if actions.contains("*") || actions.contains(&required.action) {
                return true;
            }
        }

        // Check for a specific resource, e.g., "projects:*" or "projects:delete"
        if let Some(actions) = self.granted.get(&required.resource) {
            if actions.contains("*") || actions.contains(&required.action) {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_permission_try_from_valid_string() -> Result<()> {
        let perm: PermissionRule = "project:create".try_into()?;
        assert_eq!(perm.resource, "project");
        assert_eq!(perm.action, "create");

        let perm_wildcard: PermissionRule = "*:read".try_into()?;
        assert_eq!(perm_wildcard.resource, "*");
        assert_eq!(perm_wildcard.action, "read");

        let perm_full_wildcard: PermissionRule = "*:*".try_into()?;
        assert_eq!(perm_full_wildcard.resource, "*");
        assert_eq!(perm_full_wildcard.action, "*");

        // Test TryFrom<String>
        let perm_string: PermissionRule = String::from("users:delete").try_into()?;
        assert_eq!(perm_string.resource, "users");
        assert_eq!(perm_string.action, "delete");

        Ok(())
    }

    #[test]
    fn test_permission_try_from_invalid_string() {
        // No delimiter
        let result = PermissionRule::try_from("project_create");
        assert!(result.is_err());
        if let Err(CoreError::ParseError(msg)) = result {
            assert!(msg.contains("must contain a ':'"));
        }

        // Empty resource
        let result = PermissionRule::try_from(":create");
        assert!(result.is_err());
        if let Err(CoreError::ParseError(msg)) = result {
            assert!(msg.contains("cannot have empty parts"));
        }

        // Empty action
        let result = PermissionRule::try_from("project:");
        assert!(result.is_err());
        if let Err(CoreError::ParseError(msg)) = result {
            assert!(msg.contains("cannot have empty parts"));
        }
    }

    #[test]
    fn test_permission_matches_str_logic() {
        let perm = PermissionRule {
            resource: "r".to_string(),
            action: "a".to_string(),
        };

        // Exact match
        assert_eq!(
            PermissionRule::matches_str("read", "read"),
            true,
            "Exact match failed"
        );

        // Wildcard granted
        assert_eq!(
            PermissionRule::matches_str("*", "read"),
            true,
            "Wildcard granted failed"
        );

        assert_eq!(
            PermissionRule::matches_str("read", "*"),
            true,
            "Wildcard required failed"
        );
    }

    #[test]
    fn test_permission_full_matches() -> Result<()> {
        let granted_read: PermissionRule = "project:read".try_into()?;
        let granted_all: PermissionRule = "project:*".try_into()?;
        let granted_global: PermissionRule = "*:read".try_into()?;
        let granted_everything: PermissionRule = "*:*".try_into()?;

        let required_read: PermissionRule = "project:read".try_into()?;
        let required_delete: PermissionRule = "project:delete".try_into()?;
        let required_account: PermissionRule = "account:read".try_into()?;
        let required_wildcard: PermissionRule = "project:*".try_into()?;

        // Exact Match
        assert!(granted_read.matches(&required_read));
        assert!(!granted_read.matches(&required_delete));

        // Action Wildcard Granted (project:*)
        assert!(granted_all.matches(&required_read));
        assert!(granted_all.matches(&required_delete));
        assert!(!granted_all.matches(&required_account)); // Resource mismatch

        // Resource Wildcard Granted (*:read)
        assert!(granted_global.matches(&required_read));
        assert!(granted_global.matches(&required_account));
        assert!(!granted_global.matches(&required_delete)); // Action mismatch

        // Global Wildcard Granted (*:*)
        assert!(granted_everything.matches(&required_read));
        assert!(granted_everything.matches(&required_delete));
        assert!(granted_everything.matches(&required_account));

        // Required Wildcard (project:*) - Granted covers required wildcard
        assert!(granted_all.matches(&required_wildcard));
        assert!(granted_everything.matches(&required_wildcard));
        assert!(granted_read.matches(&required_wildcard));

        Ok(())
    }

    // --- PermissionEngine Tests ---

    fn setup_checker() -> CoreResult<PermissionEngine> {
        PermissionEngine::from_str_slice(&["project:read", "project:create", "account:*", "*:read"])
    }

    #[test]
    fn test_checker_construction_and_extend() -> Result<()> {
        let mut checker = PermissionEngine::from_str_slice(&["project:read"])?;

        // Check initial state
        assert_eq!(checker.granted.len(), 1);
        assert!(checker.granted.get("project").unwrap().contains("read"));

        // Extend with new and existing permissions
        let new_perms: Vec<PermissionRule> = vec![
            "project:delete".try_into()?,
            "account:read".try_into()?,
            "project:read".try_into()?, // Duplicate, should be ignored
        ];
        checker.extend(new_perms);

        // Check extended state
        assert_eq!(checker.granted.len(), 2);
        assert!(checker.granted.get("project").unwrap().contains("read"));
        assert!(checker.granted.get("project").unwrap().contains("delete"));
        assert!(checker.granted.get("account").unwrap().contains("read"));

        Ok(())
    }

    #[test]
    fn test_checker_is_allowed_specific_match() -> Result<()> {
        let checker = setup_checker()?;
        let required: PermissionRule = "project:create".try_into()?;

        assert!(
            checker.is_allowed(&required),
            "Specific action allowed failed, project:create"
        );

        let required_denied: PermissionRule = "project:delete".try_into()?;
        assert!(
            !checker.is_allowed(&required_denied),
            "Specific action denied failed, project:delete"
        );

        Ok(())
    }

    #[test]
    fn test_checker_is_allowed_action_wildcard_grant() -> Result<()> {
        let checker = setup_checker()?;
        // Granted: account:*
        let required_delete: PermissionRule = "account:delete".try_into()?;
        assert!(
            checker.is_allowed(&required_delete),
            "Action wildcard grant failed"
        );

        Ok(())
    }

    #[test]
    fn test_checker_is_allowed_resource_wildcard_grant() -> Result<()> {
        let checker = setup_checker()?;
        // Granted: *:read
        let required_read: PermissionRule = "workspace:read".try_into()?;
        assert!(
            checker.is_allowed(&required_read),
            "Resource wildcard grant failed"
        );

        let required_write: PermissionRule = "workspace:write".try_into()?;
        assert!(
            !checker.is_allowed(&required_write),
            "Resource wildcard grant too broad failed"
        );

        Ok(())
    }

    #[test]
    fn test_checker_is_allowed_global_wildcard_grant() -> Result<()> {
        let mut checker = setup_checker()?;
        checker.extend(vec!["*".try_into()?]);
        // Granted: * (which is *:*)
        let required_write: PermissionRule = "workspace:write".try_into()?;
        assert!(
            checker.is_allowed(&required_write),
            "Global wildcard grant failed"
        );

        let required_admin: PermissionRule = "admin:do_anything".try_into()?;
        assert!(
            checker.is_allowed(&required_admin),
            "Global wildcard grant extreme case failed"
        );

        Ok(())
    }

    #[test]
    fn test_checker_is_allowed_no_match() -> Result<()> {
        let mut checker = setup_checker()?;
        // project:delete is neither granted specifically, nor covered by *:read, account:*, or project:read/create
        let required: PermissionRule = "project:delete".try_into()?;
        assert!(
            !checker.is_allowed(&required),
            "Unmatched permission check failed"
        );

        let required_unrelated: PermissionRule = "files:upload".try_into()?;
        checker.extend(vec!["*".try_into()?]);

        assert!(
            checker.is_allowed(&required_unrelated),
            "Global wildcard check failed (should pass due to '*')"
        );

        // Set up a checker with no wildcards
        let checker_strict = PermissionEngine::from_str_slice(&["users:list"])?;
        let required_files: PermissionRule = "files:upload".try_into()?;
        assert!(
            !checker_strict.is_allowed(&required_files),
            "No match should result in false"
        );

        Ok(())
    }

    #[test]
    fn test_checker_has_subset_check() -> Result<()> {
        let checker = setup_checker()?;

        let allowed_subset = PermissionRule::perms_from_str_slice(&[
            "project:read",   // Specific match
            "account:update", // Action wildcard match (account:*)
            "files:read",     // Resource wildcard match (*:read)
        ])?;
        assert!(
            checker.has_subset(&allowed_subset),
            "Allowed subset check failed"
        );

        let denied_subset = PermissionRule::perms_from_str_slice(&[
            "project:read",
            "project:delete", // Fails (not granted and no covering wildcard)
        ])?;
        assert!(
            !checker.has_subset(&denied_subset),
            "Denied subset check failed"
        );

        Ok(())
    }
}
