use uuid::Uuid;

use crate::core::{
    error::{CoreError, CoreResult},
    traits::params::ValidateParams,
};

/// Validated params for account registration.
///
/// Requires at least one of `workspace_id` or `workspace_slug`. Fails if neither
/// is provided — registration must be explicitly scoped to a workspace.
pub struct RegisterParams {
    pub email: String,
    pub password: String,
    pub name: Option<String>,
    pub workspace_id: String,
}

impl ValidateParams for RegisterParams {
    fn validate(self) -> CoreResult<Self> {
        let email = self.email.trim().to_lowercase();
        if email.is_empty() {
            return Err(CoreError::InvalidParams("email required".to_string()));
        }
        if self.password.is_empty() {
            return Err(CoreError::InvalidParams("password required".to_string()));
        }
        if self.workspace_id.is_none() {
            return Err(CoreError::InvalidParams(
                "workspaceId or workspaceSlug required".to_string(),
            ));
        }
        Ok(Self { email, ..self })
    }
}
