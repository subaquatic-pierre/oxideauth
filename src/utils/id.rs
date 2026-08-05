use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::entities::{
    audit::{AuditFields, AuditMeta},
    id::DbId,
};

// Helper to convert DbId to Uuid for Option fields
pub fn map_optional_db_id(db_id: Option<DbId>) -> Option<Uuid> {
    db_id.map(|id| id.into())
}
