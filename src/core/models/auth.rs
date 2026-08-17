use uuid::Uuid;

use crate::core::{
    error::{CoreError, CoreResult},
    models::{account::AccountDescribeParams, workspace::WorkspaceDescribeParams},
    traits::params::ValidateParams,
};

/// Validated params for account registration.
///
/// `workspace` is a typed id-or-slug descriptor. When a descriptor is present
/// it is authoritative; when absent the service falls back to the request
/// context's workspace (`ctx.ws_cache`).
pub struct RegisterParams {
    pub email: String,
    pub password: String,
    pub name: Option<String>,
    pub workspace: WorkspaceDescribeParams,
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

/// Validated params for account login.
///
/// Login is scoped to a workspace: `workspace` is a typed id-or-slug
/// descriptor that must resolve to an existing workspace.
#[derive(Debug)]
pub struct LoginParams {
    pub email: String,
    pub password: String,
    pub workspace: WorkspaceDescribeParams,
}

impl ValidateParams for LoginParams {
    fn validate(self) -> CoreResult<Self> {
        let email = self.email.trim().to_lowercase();
        if email.is_empty() {
            return Err(CoreError::InvalidParams("email required".to_string()));
        }
        if self.password.is_empty() {
            return Err(CoreError::InvalidParams("password required".to_string()));
        }
        Ok(Self {
            email,
            password: self.password,
            workspace: self.workspace,
        })
    }
}

/// Validated params for token refresh.
pub struct RefreshParams {
    pub token: String,
}

/// Validated params for requesting a password reset.
///
/// `account` is a typed id-or-email descriptor; at least one of `id` or `email`
/// must be provided.
pub struct ResetPasswordParams {
    pub account: AccountDescribeParams,
}

impl ValidateParams for ResetPasswordParams {
    fn validate(self) -> CoreResult<Self> {
        if self.account.id.is_none() && self.account.email.is_none() {
            return Err(CoreError::InvalidParams("ID or email required".to_string()));
        }
        Ok(self)
    }
}

/// Validated params for updating an account's password from a reset token.
pub struct UpdatePasswordParams {
    pub token: String,
    pub new_password: String,
}

impl ValidateParams for UpdatePasswordParams {
    fn validate(self) -> CoreResult<Self> {
        if self.new_password.is_empty() {
            return Err(CoreError::InvalidParams("password required".to_string()));
        }
        Ok(self)
    }
}

/// Validated params for confirming an account.
pub struct ConfirmParams {
    pub token: String,
}

/// Validated params for resending an account confirmation email.
///
/// `account` is a typed id-or-email descriptor; at least one of `id` or `email`
/// must be provided.
pub struct ResendConfirmParams {
    pub account: AccountDescribeParams,
}

impl ValidateParams for ResendConfirmParams {
    fn validate(self) -> CoreResult<Self> {
        if self.account.id.is_none() && self.account.email.is_none() {
            return Err(CoreError::InvalidParams("ID or email required".to_string()));
        }
        Ok(self)
    }
}

/// Validated params for revoking a token.
pub struct RevokeParams {
    pub token: String,
}

/// Validated params for initiating a Google OAuth2 login flow.
pub struct OAuthInitiateParams {
    pub redirect_url: String,
    pub workspace_id: Uuid,
}

/// Validated params for processing a Google OAuth2 callback.
pub struct OAuthCallbackParams {
    pub code: String,
    pub state: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(email: &str, password: &str) -> RegisterParams {
        RegisterParams {
            email: email.to_string(),
            password: password.to_string(),
            name: Some("User".to_string()),
            workspace: WorkspaceDescribeParams {
                id: None,
                slug: Some("ws-1".to_string()),
            },
        }
    }

    #[test]
    fn test_register_params_validate_trims_and_lowercases_email() {
        let params = params("  User@Example.COM ", "secret");
        let params = params.validate().expect("valid params should validate");
        assert_eq!(params.email, "user@example.com");
        assert_eq!(params.password, "secret");
        assert_eq!(params.name.as_deref(), Some("User"));
        assert_eq!(params.workspace.slug.as_deref(), Some("ws-1"));
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

    fn login_params(email: &str, password: &str) -> LoginParams {
        LoginParams {
            email: email.to_string(),
            password: password.to_string(),
            workspace: WorkspaceDescribeParams {
                id: None,
                slug: Some("ws-1".to_string()),
            },
        }
    }

    #[test]
    fn test_login_params_validate_trims_and_lowercases_email() {
        let params = login_params("  User@Example.COM ", "secret");
        let params = params.validate().expect("valid params should validate");
        assert_eq!(params.email, "user@example.com");
        assert_eq!(params.password, "secret");
        assert_eq!(params.workspace.slug.as_deref(), Some("ws-1"));
    }

    #[test]
    fn test_login_params_validate_whitespace_only_email_fails() {
        let params = login_params("   ", "secret");
        let err = params.validate().err().expect("expected validation error");
        assert!(matches!(err, CoreError::InvalidParams(ref msg) if msg == "email required"));
    }

    #[test]
    fn test_login_params_validate_empty_password_fails() {
        let params = login_params("user@example.com", "");
        let err = params.validate().err().expect("expected validation error");
        assert!(matches!(err, CoreError::InvalidParams(ref msg) if msg == "password required"));
    }
}
