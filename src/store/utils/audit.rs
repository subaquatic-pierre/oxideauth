use modql::field::{SeaField, SeaFields};
use sea_query::Iden;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::entities::{audit::AuditIden, workspace::WorkspaceIden};

pub fn prepare_audit_fields(fields: &mut SeaFields, user_id: Uuid, is_create: bool) {
    let now = OffsetDateTime::now_utc();

    if is_create {
        fields.push(SeaField::new(AuditIden::CreatedBy, user_id));
        fields.push(SeaField::new(AuditIden::CreatedAt, now));
    } else {
        fields.push(SeaField::new(AuditIden::UpdatedBy, user_id));
        fields.push(SeaField::new(AuditIden::UpdatedAt, now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_query::Value as SeaValue;
    use uuid::Uuid;

    fn iden_names(fields: &SeaFields) -> Vec<String> {
        fields
            .clone()
            .into_vec()
            .iter()
            .map(|f| f.iden.to_string())
            .collect()
    }

    #[test]
    fn test_prepare_audit_fields_create() {
        // -- Setup
        let user_id = Uuid::new_v4();
        let mut fields = SeaFields::new(vec![]);

        // -- Execute
        prepare_audit_fields(&mut fields, user_id, true);

        // -- Assert
        let names = iden_names(&fields);
        assert!(names.contains(&"created_by".to_string()));
        assert!(names.contains(&"created_at".to_string()));
        assert!(!names.contains(&"updated_by".to_string()));
        assert!(!names.contains(&"updated_at".to_string()));

        // The created_by field must carry the user id
        let vec = fields.clone().into_vec();
        let created_by = vec
            .iter()
            .find(|f| f.iden.to_string() == "created_by")
            .unwrap();
        assert_eq!(created_by.sea_value(), Some(&SeaValue::Uuid(Some(user_id))));
    }

    #[test]
    fn test_prepare_audit_fields_update() {
        // -- Setup
        let user_id = Uuid::new_v4();
        let mut fields = SeaFields::new(vec![]);

        // -- Execute
        prepare_audit_fields(&mut fields, user_id, false);

        // -- Assert
        let names = iden_names(&fields);
        assert!(names.contains(&"updated_by".to_string()));
        assert!(names.contains(&"updated_at".to_string()));
        assert!(!names.contains(&"created_by".to_string()));
        assert!(!names.contains(&"created_at".to_string()));

        // The updated_by field must carry the user id
        let vec = fields.clone().into_vec();
        let updated_by = vec
            .iter()
            .find(|f| f.iden.to_string() == "updated_by")
            .unwrap();
        assert_eq!(updated_by.sea_value(), Some(&SeaValue::Uuid(Some(user_id))));
    }
}
