use modql::field::{SeaField, SeaFields};
use sea_query::Iden;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::entities::{audit::AuditIden, workspace::WorkspaceIden};

pub fn prepare_workspace_scope(fields: SeaFields, workspace_id: Option<Uuid>) -> SeaFields {
    if let Some(workspace_id) = workspace_id {
        // 1. Convert the list to a mutable vector for filtering.
        let mut field_vec = fields.into_vec();

        // The identifier string we need to filter out.
        let ws_iden_str = WorkspaceIden::WorkspaceId.to_string();

        // 2. CRITICAL STEP: Retain all fields *except* the one for workspace_id.
        // This ensures we eliminate any workspace_id that came from the DTO.
        field_vec.retain(|field| field.iden.to_string() != ws_iden_str);

        // 3. Append the secure, context-enforced workspace_id.
        let ws_field = SeaField::new(WorkspaceIden::WorkspaceId, workspace_id);
        field_vec.push(ws_field);

        // 4. Convert the safe, unique list back into the SeaFields wrapper.
        return SeaFields::new(field_vec);
    }

    fields
}

#[cfg(test)]
mod tests {
    use crate::store::entities::permission::PermissionForCreate;
    use anyhow::Result;
    use modql::field::HasSeaFields;

    use super::*;
    use sea_query::Iden;

    // NOTE: This must match the Iden used in your prepare_workspace_scope implementation
    #[derive(Iden, Copy, Clone)]
    pub enum WorkspaceIden {
        WorkspaceId,
    }

    #[test]
    fn test_prepare_workspace_scope_enforces_context_id() -> Result<()> {
        // --- Scenario 1: Enforcement (Overwrite) ---

        // Arrange
        let enforced_ws_id = Uuid::new_v4();
        let dto_ws_id = Uuid::new_v4(); // Different ID coming from the DTO
        assert_ne!(enforced_ws_id, dto_ws_id);

        let mut data = PermissionForCreate::default();
        data.workspace_id = Some(dto_ws_id);

        let initial_fields = data.not_none_sea_fields();
        let initial_len = initial_fields.clone().into_vec().len();

        // Act
        // Pass the fields (containing dto_ws_id) and the enforced context ID
        let scoped_fields = prepare_workspace_scope(initial_fields, Some(enforced_ws_id));

        // Assert
        let final_vec = scoped_fields.into_vec();

        // 1. Length must be the same (one element replaced)
        assert_eq!(
            final_vec.len(),
            initial_len,
            "Length should not change (overwrite)"
        );

        // 2. Check that the final list contains the ENFORCED ID, not the DTO's ID.
        let ws_iden_str = WorkspaceIden::WorkspaceId.to_string();

        let final_ws_field = final_vec.iter().find(|f| f.iden.to_string() == ws_iden_str);

        assert!(final_ws_field.is_some(), "Workspace ID must be present");

        // Check the value against the enforced ID
        // Note: Checking the value requires converting the SimpleExpr back,
        // which is cumbersome. We rely on the Rust type match here if possible,
        // or check its representation if necessary. For simplicity, we check
        // that the resulting field is not the old ID's raw representation.

        // We can check the raw value representation if we assume SimpleExpr from Uuid is predictable.
        let expected_field = SeaField::new(WorkspaceIden::WorkspaceId, enforced_ws_id);

        // Check if the resulting field matches the expected (enforced) field
        let expected_val = expected_field.value;
        let final_val = final_ws_field.cloned().unwrap().value;

        assert_eq!(
            expected_val, final_val,
            "The final workspace_id must match the context's enforced ID."
        );

        Ok(())
    }

    #[test]
    fn test_prepare_workspace_scope_injects_if_missing() -> Result<()> {
        // --- Scenario 2: Injection (No DTO ID) ---

        // Arrange
        let enforced_ws_id = Uuid::new_v4();

        // Create a DTO that manually excludes workspace_id (simulated scenario)
        let mut data = PermissionForCreate::default();
        let mut initial_fields = data.not_none_sea_fields().into_vec();

        // Manually remove workspace_id field from the list to simulate a DTO
        // that omits it, which is valid for HasSeaFields in some modql setups.
        let ws_iden_str = WorkspaceIden::WorkspaceId.to_string();
        initial_fields.retain(|f| f.iden.to_string() != ws_iden_str);

        let initial_fields = SeaFields::new(initial_fields.clone());
        let initial_len = initial_fields.clone().into_vec().len(); // Length without workspace_id

        // Act
        let scoped_fields = prepare_workspace_scope(initial_fields, Some(enforced_ws_id));

        // Assert
        let final_vec = scoped_fields.into_vec();

        // 1. Length must increase by one (one element injected)
        assert_eq!(
            final_vec.len(),
            initial_len + 1,
            "Length should increase by one (injection)"
        );

        // 2. Check that the final list contains the ENFORCED ID.
        let ws_iden_str = WorkspaceIden::WorkspaceId.to_string();
        let final_ws_field = final_vec.iter().find(|f| f.iden.to_string() == ws_iden_str);

        assert!(
            final_ws_field.is_some(),
            "Workspace ID must be present after injection"
        );

        Ok(())
    }

    #[test]
    fn test_prepare_workspace_scope_no_scope_pass_through() -> Result<()> {
        // --- Scenario 3: No Scoping (Pass-through) ---

        // Arrange
        let dto_ws_id = Uuid::new_v4();
        let mut data = PermissionForCreate::default();
        data.workspace_id = Some(dto_ws_id);
        let initial_fields = data.not_none_sea_fields();

        let initial_vec = initial_fields.into_vec();
        let initial_len = initial_vec.len();

        // Act
        // Pass the fields and a None context scope
        let scoped_fields = prepare_workspace_scope(SeaFields::new(initial_vec), None);

        // Assert
        let final_vec = scoped_fields.into_vec();

        // 1. Length must be the same
        assert_eq!(final_vec.len(), initial_len, "Length must be unchanged");

        // 2. Check that the original DTO ID remains.
        let ws_iden_str = WorkspaceIden::WorkspaceId.to_string();
        let final_ws_field = final_vec.iter().find(|f| f.iden.to_string() == ws_iden_str);

        let expected_field = SeaField::new(WorkspaceIden::WorkspaceId, dto_ws_id);

        // Check if the resulting field matches the expected (enforced) field
        let expected_val = expected_field.value;
        let final_val = final_ws_field.cloned().unwrap().value;

        assert_eq!(
            expected_val, final_val,
            "The final workspace_id must match the context's enforced ID."
        );

        Ok(())
    }
}
