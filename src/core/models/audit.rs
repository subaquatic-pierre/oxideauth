use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    store::entities::{
        audit::{AuditFields, AuditMeta},
        id::DbId,
    },
    utils::id::map_optional_db_id,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CoreAuditFields {
    pub created_by: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub updated_by: Option<Uuid>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
    pub meta: AuditMeta,
}

impl From<AuditFields> for CoreAuditFields {
    fn from(value: AuditFields) -> Self {
        Self {
            created_by: value.created_by.into(),
            created_at: value.created_at,
            updated_by: map_optional_db_id(value.updated_by),
            updated_at: value.updated_at,
            meta: value.meta,
        }
    }
}

impl Default for CoreAuditFields {
    fn default() -> Self {
        Self {
            created_by: Uuid::nil(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_by: None,
            updated_at: None,
            meta: AuditMeta::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    #[test]
    fn test_core_audit_fields_from_audit_fields() {
        let created_by = Uuid::new_v4();
        let updated_by = Uuid::new_v4();
        let updated_at = OffsetDateTime::UNIX_EPOCH + Duration::seconds(60);

        let fields = AuditFields {
            created_by: created_by.into(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_by: Some(updated_by.into()),
            updated_at: Some(updated_at),
            meta: AuditMeta {
                schema_version: "3".to_string(),
            },
        };

        let core: CoreAuditFields = fields.into();
        assert_eq!(core.created_by, created_by);
        assert_eq!(core.created_at, OffsetDateTime::UNIX_EPOCH);
        assert_eq!(core.updated_by, Some(updated_by));
        assert_eq!(core.updated_at, Some(updated_at));
        assert_eq!(core.meta.schema_version, "3");
    }

    #[test]
    fn test_core_audit_fields_from_audit_fields_with_no_updater() {
        let created_by = Uuid::new_v4();

        let fields = AuditFields {
            created_by: created_by.into(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_by: None,
            updated_at: None,
            meta: AuditMeta::default(),
        };

        let core: CoreAuditFields = fields.into();
        assert_eq!(core.created_by, created_by);
        assert!(core.updated_by.is_none());
        assert!(core.updated_at.is_none());
    }

    #[test]
    fn test_core_audit_fields_default() {
        let audit = CoreAuditFields::default();
        assert_eq!(audit.created_by, Uuid::nil());
        assert_eq!(audit.created_at, OffsetDateTime::UNIX_EPOCH);
        assert!(audit.updated_by.is_none());
        assert!(audit.updated_at.is_none());
        assert_eq!(audit.meta.schema_version, "");
    }
}
