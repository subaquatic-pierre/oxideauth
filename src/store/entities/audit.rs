use modql::field::Fields;
use modql::filter::{FilterNodes, OpValsValue};
use sea_query::Iden;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::entities::id::DbId;
use crate::store::utils::time_to_sea_value;

#[derive(Iden, Copy, Clone)]
pub enum AuditIden {
    CreatedBy,
    CreatedAt,
    UpdatedBy,
    UpdatedAt,
}

#[derive(Debug, FromRow, Deserialize)]
pub struct AuditFields {
    pub created_by: DbId,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub updated_by: Option<DbId>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,

    #[serde(rename = "audit")]
    #[sqlx(json)]
    pub meta: AuditMeta,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AuditMeta {
    pub schema_version: String,
}

impl Default for AuditFields {
    fn default() -> Self {
        Self {
            // Use the Unix epoch (1970-01-01 00:00:00 UTC) as the default time
            created_at: OffsetDateTime::UNIX_EPOCH,

            created_by: DbId::default(),
            meta: AuditMeta::default(),

            updated_by: None,
            updated_at: None,
        }
    }
}
