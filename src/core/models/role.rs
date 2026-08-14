use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{
    core::{
        models::permission::{PermissionEngine, PermissionRule},
        traits::params::ValidateParams,
    },
    store::entities::{
        audit::AuditMeta, permission::PermissionMeta, role::RoleForCreate, role::RoleForUpdate,
    },
};

use modql::filter::OpValString;
use uuid::Uuid;

use crate::{
    core::{
        error::{CoreError, CoreResult},
        models::{
            audit::CoreAuditFields,
            list::{RequestFilterParams, RequestListOptions},
            permission::Permission,
            workspace::Workspace,
        },
        traits::{filter::OpValWorkspaceId, list::RequestListParams},
    },
    store::entities::role::{
        JoinedPermissionOnRole, RoleFilter as StoreRoleFilter, RoleMeta as StoreRoleMeta, RoleRow,
        RoleWithPermissions,
    },
};

pub type RoleMeta = StoreRoleMeta;
pub type RoleFilter = StoreRoleFilter;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Role {
    pub id: Uuid,
    pub workspace_id: Uuid,

    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<Permission>,

    pub tags: Vec<String>,
    pub meta: RoleMeta,

    pub audit: CoreAuditFields,
}

impl From<RoleWithPermissions> for Role {
    fn from(rwp: RoleWithPermissions) -> Self {
        let row = rwp.role;
        let permissions = rwp
            .permissions
            .into_iter()
            .map(|el| Permission {
                id: el.id.into(),
                workspace_id: el.workspace_id,
                name: el.name,
                description: el.description,
                tags: el.tags,
                meta: PermissionMeta { ..el.meta },
                audit: CoreAuditFields {
                    created_by: el.created_by.into(),
                    created_at: el.created_at,
                    updated_by: el.updated_by.map(|el| el.into()),
                    updated_at: el.updated_at,
                    meta: AuditMeta {
                        schema_version: "DO_NOT_USE".into(),
                    },
                },
            })
            .collect();

        Self {
            id: row.id.into(),
            workspace_id: row.workspace_id,
            name: row.name,
            description: row.description,
            permissions,
            tags: row.tags,
            meta: row.meta,
            audit: row.audit.into(),
        }
    }
}

impl Default for Role {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace_id: Uuid::nil(),
            name: "New Role".to_string(),
            description: None,
            permissions: vec![],
            tags: vec![],
            meta: RoleMeta {
                schema_version: "1".to_string(),
            },
            audit: CoreAuditFields::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RoleCreateParams {
    pub workspace_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub permission_ids: Vec<Uuid>,
    pub tags: Vec<String>,
    pub meta: RoleMeta,
}

impl From<RoleCreateParams> for RoleForCreate {
    fn from(params: RoleCreateParams) -> Self {
        Self {
            workspace_id: params.workspace_id,
            name: params.name,
            description: params.description,
            tags: params.tags,
            meta: params.meta,
        }
    }
}

impl RoleCreateParams {
    pub fn new_workspace_system_role(
        ws_id: Uuid,
        name: &str,
        desc: Option<&str>,
        perm_ids: Vec<Uuid>,
    ) -> Self {
        Self {
            workspace_id: ws_id,
            name: name.to_string(),
            description: desc.map(|d| d.to_string()),
            permission_ids: perm_ids,
            tags: vec!["system".to_string()],
            meta: RoleMeta::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RoleUpdateParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: Option<String>,
    pub description: Option<String>,
    pub permission_ids: Option<Vec<Uuid>>, // To sync the join table
    pub tags: Option<Vec<String>>,
    pub meta: Option<RoleMeta>,
}

impl From<RoleUpdateParams> for RoleForUpdate {
    fn from(params: RoleUpdateParams) -> Self {
        Self {
            name: params.name,
            description: params.description,
            tags: params.tags,
            meta: params.meta,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RoleDescribeParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

pub struct RoleDeleteParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

pub struct RoleListParams {
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<RoleFilter>>,
    pub options: Option<RequestListOptions>,
}

impl RequestListParams<RoleFilter> for RoleListParams {
    fn filter(&self) -> Option<RequestFilterParams<RoleFilter>> {
        self.filter.clone()
    }

    fn options(&self) -> Option<RequestListOptions> {
        self.options.clone()
    }
}

impl OpValWorkspaceId for RoleFilter {
    fn get_workspace_id_opval(&self) -> Option<&OpValString> {
        self.workspace_id
            .as_ref()
            .and_then(|op_vals| op_vals.0.first())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RoleCheck {
    name: String,
    permissions: RolePermissions,
}

impl RoleCheck {
    pub fn new(name: &str, perms: &[PermissionRule]) -> Self {
        Self {
            name: name.to_string(),
            permissions: RolePermissions::new(perms),
        }
    }

    pub fn permissions(&self) -> &RolePermissions {
        &self.permissions
    }

    pub fn extend_permissions(&mut self, perms: &[PermissionRule]) {
        self.permissions.extend(perms);
    }

    pub fn remove_permissions(&mut self, perms: &[PermissionRule]) {
        self.permissions.remove(perms);
    }

    pub fn build_permission_checker(&self) -> PermissionEngine {
        self.permissions.build_checker()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RolePermissions {
    perms: HashSet<PermissionRule>,
}

impl RolePermissions {
    pub fn new(perms: &[PermissionRule]) -> Self {
        let mut _self = Self {
            perms: HashSet::new(),
        };

        _self.extend(perms);

        _self
    }

    pub fn extend(&mut self, perms: &[PermissionRule]) {
        for perm in perms {
            self.perms.insert(perm.clone());
        }
    }

    pub fn remove(&mut self, perms: &[PermissionRule]) {
        for perm in perms {
            self.perms.remove(&perm);
        }
    }

    pub fn build_checker(&self) -> PermissionEngine {
        let size = self.perms.len();
        let mut perms: Vec<PermissionRule> = Vec::with_capacity(size);
        for perm in self.perms.iter() {
            perms.push(perm.clone())
        }

        PermissionEngine::new(perms)
    }
}

impl Iterator for RolePermissions {
    type Item = PermissionRule;

    fn next(&mut self) -> Option<Self::Item> {
        self.perms.iter().next().cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use crate::store::entities::audit::{AuditFields, AuditMeta};
    use time::OffsetDateTime;

    fn make_role_with_permissions() -> RoleWithPermissions {
        let id = Uuid::new_v4();
        let ws_id = Uuid::new_v4();
        let role = RoleRow {
            id: id.into(),
            workspace_id: ws_id,
            name: "Admin".to_string(),
            description: Some("admin role".to_string()),
            tags: vec!["system".to_string()],
            meta: RoleMeta {
                schema_version: "1".to_string(),
            },
            audit: AuditFields {
                created_by: id.into(),
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_by: None,
                updated_at: None,
                meta: AuditMeta::default(),
            },
        };
        let perm = JoinedPermissionOnRole {
            id: id.into(),
            workspace_id: ws_id,
            name: "project:read".to_string(),
            code: Some("code".to_string()),
            description: Some("desc".to_string()),
            tags: vec![],
            meta: PermissionMeta {
                schema_version: "1".to_string(),
            },
            created_by: id.into(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_by: Some(id.into()),
            updated_at: Some(OffsetDateTime::UNIX_EPOCH),
        };
        RoleWithPermissions {
            id: id.into(),
            role,
            permissions: vec![perm],
        }
    }

    #[test]
    fn test_role_from_role_with_permissions() {
        let rwp = make_role_with_permissions();
        let role: Role = rwp.into();

        assert_eq!(role.name, "Admin");
        assert_eq!(role.description.as_deref(), Some("admin role"));
        assert_eq!(role.tags, vec!["system".to_string()]);
        assert_eq!(role.meta.schema_version, "1");
        assert_eq!(role.audit.created_at, OffsetDateTime::UNIX_EPOCH);
        assert_eq!(role.permissions.len(), 1);

        let perm = &role.permissions[0];
        assert_eq!(perm.name, "project:read");
        assert_eq!(perm.description.as_deref(), Some("desc"));
        assert_eq!(perm.meta.schema_version, "1");
        // permission audit meta is force-marked in the conversion
        assert_eq!(perm.audit.meta.schema_version, "DO_NOT_USE");
        assert_eq!(perm.audit.updated_by, Some(role.id));
    }

    #[test]
    fn test_role_default() {
        let role = Role::default();
        assert_eq!(role.name, "New Role");
        assert_eq!(role.workspace_id, Uuid::nil());
        assert!(role.description.is_none());
        assert!(role.permissions.is_empty());
        assert!(role.tags.is_empty());
        assert_eq!(role.meta.schema_version, "1");
        assert_eq!(role.audit.created_by, Uuid::nil());
    }

    #[test]
    fn test_role_create_params_into_store() {
        let ws_id = Uuid::new_v4();
        let params = RoleCreateParams {
            workspace_id: ws_id,
            name: "Editor".to_string(),
            description: Some("d".to_string()),
            permission_ids: vec![Uuid::new_v4()],
            tags: vec!["t".to_string()],
            meta: RoleMeta {
                schema_version: "1".to_string(),
            },
        };

        let store: RoleForCreate = params.into();
        assert_eq!(store.workspace_id, ws_id);
        assert_eq!(store.name, "Editor");
        assert_eq!(store.description.as_deref(), Some("d"));
        assert_eq!(store.tags, vec!["t".to_string()]);
        assert_eq!(store.meta.schema_version, "1");
    }

    #[test]
    fn test_role_new_workspace_system_role() {
        let ws_id = Uuid::new_v4();
        let perm_ids = vec![Uuid::new_v4(), Uuid::new_v4()];
        let params = RoleCreateParams::new_workspace_system_role(
            ws_id,
            "system-owner",
            Some("system role"),
            perm_ids.clone(),
        );

        assert_eq!(params.workspace_id, ws_id);
        assert_eq!(params.name, "system-owner");
        assert_eq!(params.description.as_deref(), Some("system role"));
        assert_eq!(params.permission_ids, perm_ids);
        assert_eq!(params.tags, vec!["system".to_string()]);
        assert_eq!(params.meta.schema_version, "");

        // None description variant
        let params = RoleCreateParams::new_workspace_system_role(ws_id, "r", None, vec![]);
        assert!(params.description.is_none());
    }

    #[test]
    fn test_role_update_params_into_store() {
        let params = RoleUpdateParams {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            name: Some("N".to_string()),
            description: Some("d".to_string()),
            permission_ids: Some(vec![Uuid::new_v4()]),
            tags: Some(vec!["t".to_string()]),
            meta: Some(RoleMeta {
                schema_version: "2".to_string(),
            }),
        };

        let store: RoleForUpdate = params.into();
        assert_eq!(store.name.as_deref(), Some("N"));
        assert_eq!(store.description.as_deref(), Some("d"));
        assert_eq!(store.tags, Some(vec!["t".to_string()]));
        assert_eq!(store.meta.unwrap().schema_version, "2");
    }

    #[test]
    fn test_role_check_new_permissions_and_checker() -> Result<()> {
        let mut check = RoleCheck::new("dev", &["project:read".try_into()?]);

        let checker = check.build_permission_checker();
        assert!(checker.is_allowed(&"project:read".try_into()?));
        assert!(!checker.is_allowed(&"project:write".try_into()?));

        check.extend_permissions(&["project:write".try_into()?]);
        let checker = check.build_permission_checker();
        assert!(checker.is_allowed(&"project:write".try_into()?));

        check.remove_permissions(&["project:write".try_into()?]);
        let checker = check.build_permission_checker();
        assert!(!checker.is_allowed(&"project:write".try_into()?));
        assert!(checker.is_allowed(&"project:read".try_into()?));

        Ok(())
    }

    #[test]
    fn test_role_permissions_remove_unknown_perm_is_noop() -> Result<()> {
        let perms = RolePermissions::new(&["a:1".try_into()?]);
        let mut perms = perms;
        perms.remove(&["b:2".try_into()?]);
        let checker = perms.build_checker();
        assert!(checker.is_allowed(&"a:1".try_into()?));
        Ok(())
    }

    #[test]
    fn test_role_permissions_iterator() -> Result<()> {
        // The Iterator impl re-yields the first element each call
        let mut rp = RolePermissions::new(&["project:read".try_into()?]);
        let first = rp.next().expect("non-empty perms should yield");
        assert_eq!(first, "project:read".try_into()?);

        let mut empty = RolePermissions::new(&[]);
        assert!(empty.next().is_none());
        Ok(())
    }

    #[test]
    fn test_role_list_params_accessors() {
        let params = RoleListParams {
            workspace_id: Uuid::new_v4(),
            filter: None,
            options: None,
        };
        assert!(params.filter().is_none());
        assert!(params.options().is_none());
    }

    #[test]
    fn test_role_filter_workspace_id_opval() {
        use crate::core::traits::filter::OpValIsString;

        let filter = RoleFilter::default();
        assert!(filter.get_workspace_id_opval().is_none());

        let ws_id = Uuid::new_v4();
        let filter: RoleFilter = serde_json::from_value(serde_json::json!({
            "workspace_id": ws_id.to_string()
        }))
        .expect("filter should deserialize");

        let opval = filter.get_workspace_id_opval().expect("ws present");
        assert_eq!(opval.as_eq_string(), Some(ws_id.to_string().as_str()));
    }
}
