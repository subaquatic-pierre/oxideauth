use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    core::error::{CoreError, CoreResult},
    store::entities::{
        audit::{AuditFields, AuditMeta},
        id::DbId,
    },
};

// Helper to convert DbId to Uuid for Option fields
pub fn map_optional_db_id(db_id: Option<DbId>) -> Option<Uuid> {
    db_id.map(|id| id.into())
}

pub fn id_or_string(
    id: Option<Uuid>,
    email: Option<String>,
    msg: Option<&str>,
) -> CoreResult<String> {
    let res = id
        .map(|id| id.to_string())
        .or(email.clone())
        .ok_or(CoreError::InvalidParams(
            msg.unwrap_or("ID or identifier required").to_string(),
        ));

    res
}
