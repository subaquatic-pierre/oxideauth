use std::fmt::{self, Display};
use std::str::FromStr;

use modql::field::Fields;
use modql::filter::{FilterNodes, OpValsString, OpValsValue};
use oxideauth_macros::{EnumTextType, HasId};
use sea_query::{Iden, Nullable, Value as SeaValue};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::prelude::FromRow;
use sqlx::Type;
use strum_macros::{Display, EnumString};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::entities::audit::{AuditFields, AuditMeta};
use crate::store::entities::id::DbId;
use crate::store::entities::role::RoleMeta;
use crate::store::error::{StoreError, StoreResult};
use crate::store::traits::meta::HasId;
use crate::store::utils::{json_to_sea_value, time_to_sea_value};

#[derive(Iden, Copy, Clone)]

pub enum MembershipIden {
    #[iden = "membership"]
    Table, // TABLE_NAME
    Id, // TABLE_PK
    Tags,
    Meta,
    MembershipRole,
    MembershipId,
    Roles,
    RoleId,
    Role,
    #[iden = "id"]
    RolePk,
}

// --- Row (DB-facing) ---
/// Maps to the `membership` SQL table.
#[derive(Debug, FromRow, Deserialize, HasId)]
pub struct MembershipRow {
    pub id: DbId,

    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub scope: MembershipScope,
    pub status: MembershipStatus,
    pub project_id: Option<Uuid>,
    pub version: i64,
    pub tags: Vec<String>,
    #[sqlx(json)]
    pub meta: MembershipMeta,
    #[sqlx(flatten)]
    pub audit: AuditFields,
}

#[derive(Debug, FromRow, Deserialize, HasId)]
pub struct JoinedRoleOnMembership {
    pub id: DbId,
    pub workspace_id: Uuid,

    // Role identity
    pub name: String,
    pub description: Option<String>,

    pub tags: Vec<String>,
    #[sqlx(json)]
    pub meta: RoleMeta,

    pub created_by: DbId,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub updated_by: Option<DbId>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
}

#[derive(Debug, FromRow, Deserialize, HasId)]
pub struct MembershipWithRoles {
    pub id: DbId,
    #[sqlx(flatten)]
    pub membership: MembershipRow,
    #[sqlx(json)]
    pub roles: Vec<JoinedRoleOnMembership>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, EnumTextType)]
#[serde(rename_all = "lowercase")]
pub enum MembershipScope {
    Workspace,
    Project,
}

impl From<MembershipScope> for SeaValue {
    fn from(value: MembershipScope) -> Self {
        let s = format!("{value}");
        SeaValue::String(Some(s))
    }
}

impl Nullable for MembershipScope {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, EnumTextType)]
#[serde(rename_all = "lowercase")]
pub enum MembershipStatus {
    Invited,
    Active,
    Suspended,
}

impl From<MembershipStatus> for SeaValue {
    fn from(value: MembershipStatus) -> Self {
        let s = format!("{value}");
        SeaValue::String(Some(s))
    }
}

impl Nullable for MembershipStatus {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

#[derive(Debug, Fields)]
pub struct MembershipForCreate {
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub scope: MembershipScope,
    pub status: MembershipStatus,
    pub project_id: Option<Uuid>,
    pub tags: Vec<String>,
    pub meta: MembershipMeta,
}

#[derive(Debug, Fields, Clone)]
pub struct MembershipForUpdate {
    pub scope: Option<MembershipScope>,
    pub status: Option<MembershipStatus>,
    pub project_id: Option<Uuid>,
    pub version: Option<i64>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<MembershipMeta>,
}

#[derive(Debug, Default, Fields, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct MembershipMeta {
    pub schema_version: String,
}

impl Nullable for MembershipMeta {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

impl From<MembershipMeta> for SeaValue {
    fn from(value: MembershipMeta) -> Self {
        json_to_sea_value(serde_json::to_value(value).unwrap()).unwrap()
    }
}

/// Filtering options for `membership` queries.
#[derive(FilterNodes, Deserialize, Default, Debug, Clone)]
pub struct MembershipFilter {
    #[modql(cast_as = "uuid")]
    pub id: Option<OpValsString>,
    #[modql(cast_as = "uuid")]
    pub account_id: Option<OpValsString>,
    #[modql(cast_as = "uuid")]
    pub workspace_id: Option<OpValsString>,
    pub scope: Option<OpValsString>,
    #[modql(cast_as = "uuid")]
    pub project_id: Option<OpValsString>,
    pub status: Option<OpValsString>,

    // NOTE: Filtering on JSONB and TEXT[] fields requires custom modql logic.
    pub tags: Option<OpValsValue>,
    pub meta: Option<OpValsValue>,
    #[modql(cast_as = "uuid")]
    pub created_by: Option<OpValsString>,
    #[modql(to_sea_value_fn = "time_to_sea_value")]
    pub created_at: Option<OpValsValue>,
    #[modql(cast_as = "uuid")]
    pub updated_by: Option<OpValsString>,
    #[modql(to_sea_value_fn = "time_to_sea_value")]
    pub updated_at: Option<OpValsValue>,
}

impl TryFrom<JsonValue> for MembershipFilter {
    type Error = StoreError;

    fn try_from(value: JsonValue) -> StoreResult<Self> {
        let res = serde_json::from_value(value)?;
        Ok(res)
    }
}

// --- Defaults for testing ---
#[cfg(test)]
impl Default for MembershipForCreate {
    fn default() -> Self {
        Self {
            account_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            scope: MembershipScope::Workspace,
            project_id: None,
            status: MembershipStatus::Active,
            tags: vec![],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        }
    }
}

#[cfg(test)]
impl Default for MembershipForUpdate {
    fn default() -> Self {
        Self {
            scope: None,
            project_id: None,
            status: None,
            version: None,
            tags: None,
            meta: None,
        }
    }
}
