use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{
    core::{
        models::permission::{PermissionCheck, PermissionChecker},
        traits::params::ValidateParams,
    },
    store::entities::{audit::AuditMeta, permission::PermissionMeta, role::RoleForUpdate},
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
        RoleFilter as StoreRoleFilter, RoleMeta as StoreRoleMeta, RoleRow, RoleWithPermissions,
    },
};

pub type RoleMeta = StoreRoleMeta;
pub type RoleFilter = StoreRoleFilter;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Role {
    pub id: Uuid,
    pub workspace: Workspace,

    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<Permission>,

    pub tags: Vec<String>,
    pub meta: RoleMeta,

    pub audit: CoreAuditFields,
}

impl Role {
    pub fn from_row_with_entities(
        row_with_perms: RoleWithPermissions,
        workspace: Workspace,
    ) -> CoreResult<Self> {
        let row = row_with_perms.role;

        if row.workspace_id != workspace.id {
            return Err(CoreError::InvalidParams(
                "row.workspace_id does not match workspace.id".to_string(),
            ));
        }

        let permissions = row_with_perms
            .permissions
            .into_iter()
            .map(|el| Permission {
                id: el.id.into(),
                workspace: workspace.clone(),
                name: el.name,
                code: el.code,
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

        Ok(Self {
            id: row.id.into(),
            workspace,
            name: row.name,
            description: row.description,
            permissions,
            tags: row.tags,
            meta: row.meta,
            audit: row.audit.into(),
        })
    }
}

impl Default for Role {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace: Workspace::default(),
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
    pub fn new(name: &str, perms: &[PermissionCheck]) -> Self {
        Self {
            name: name.to_string(),
            permissions: RolePermissions::new(perms),
        }
    }

    pub fn permissions(&self) -> &RolePermissions {
        &self.permissions
    }

    pub fn extend_permissions(&mut self, perms: &[PermissionCheck]) {
        self.permissions.extend(perms);
    }

    pub fn remove_permissions(&mut self, perms: &[PermissionCheck]) {
        self.permissions.remove(perms);
    }

    pub fn build_permission_checker(&self) -> PermissionChecker {
        self.permissions.build_checker()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RolePermissions {
    perms: HashSet<PermissionCheck>,
}

impl RolePermissions {
    pub fn new(perms: &[PermissionCheck]) -> Self {
        let mut _self = Self {
            perms: HashSet::new(),
        };

        _self.extend(perms);

        _self
    }

    pub fn extend(&mut self, perms: &[PermissionCheck]) {
        for perm in perms {
            self.perms.insert(perm.clone());
        }
    }

    pub fn remove(&mut self, perms: &[PermissionCheck]) {
        for perm in perms {
            self.perms.remove(&perm);
        }
    }

    pub fn build_checker(&self) -> PermissionChecker {
        let size = self.perms.len();
        let mut perms: Vec<PermissionCheck> = Vec::with_capacity(size);
        for perm in self.perms.iter() {
            perms.push(perm.clone())
        }

        PermissionChecker::new(perms)
    }
}

impl Iterator for RolePermissions {
    type Item = PermissionCheck;

    fn next(&mut self) -> Option<Self::Item> {
        self.perms.iter().next().cloned()
    }
}
