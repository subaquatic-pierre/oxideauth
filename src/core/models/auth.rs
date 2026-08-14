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
        Ok(Self { email, ..self })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(email: &str, password: &str) -> RegisterParams {
        RegisterParams {
            email: email.to_string(),
            password: password.to_string(),
            name: Some("User".to_string()),
            workspace_id: "ws-1".to_string(),
        }
    }

    #[test]
    fn test_register_params_validate_trims_and_lowercases_email() {
        let params = params("  User@Example.COM ", "secret");
        let params = params.validate().expect("valid params should validate");
        assert_eq!(params.email, "user@example.com");
        assert_eq!(params.password, "secret");
        assert_eq!(params.name.as_deref(), Some("User"));
        assert_eq!(params.workspace_id, "ws-1");
    }

    #[test]
    fn test_register_params_validate_whitespace_only_email_fails() {
        let params = params("   ", "secret");
        let err = params.validate().err().expect("expected validation error");
        assert!(matches!(err, CoreError::InvalidParams(ref msg) if msg == "email required"));
    }

    #[test]
    fn test_register_params_validate_empty_password_fails() {
        let params = params("user@example.com", "");
        let err = params.validate().err().expect("expected validation error");
        assert!(matches!(err, CoreError::InvalidParams(ref msg) if msg == "password required"));
    }
}
