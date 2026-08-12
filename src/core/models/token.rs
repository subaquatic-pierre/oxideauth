use std::str::FromStr;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    core::error::{CoreError, CoreResult},
    utils::time::now_utc,
};

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
pub struct TokenClaims {
    pub sub: Uuid, // Account ID
    pub ws: Uuid,  // Workspace ID
    pub mem: Uuid, // Membership ID
    pub iss: String,
    pub aud: String,
    pub exp: usize,
    pub iat: usize,
    pub ty: TokenType,
    pub mem_ver: u64,      // Membership token version
    pub acc_ver: u64,      // Account token version
    pub sid: Option<Uuid>, // Session ID (None for single-use tokens)
    pub jti: Option<Uuid>, // JWT ID — unique per token
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy, Deserialize)]
pub enum TokenType {
    Auth,
    PasswordReset,
    Refresh,
    AccountConfirm,
}

impl TokenClaims {
    pub fn new(
        sub: Uuid,
        ws: Uuid,
        mem: Uuid,
        exp: OffsetDateTime,
        ty: TokenType,
        mem_ver: u64,
        acc_ver: u64,
        sid: Option<Uuid>,
        jti: Option<Uuid>,
    ) -> Self {
        Self {
            sub: sub,
            ws: ws,
            mem: mem,
            iss: "oxideauth.app".to_string(),
            aud: "oxideauth.api".to_string(),
            iat: now_utc().unix_timestamp() as usize,
            exp: exp.unix_timestamp() as usize,
            ty,
            mem_ver,
            acc_ver,
            sid,
            jti,
        }
    }

    pub fn is_expired(&self) -> bool {
        let now = now_utc().unix_timestamp() as usize;
        if self.exp < now { true } else { false }
    }

    /// The account id encoded in the token subject (`sub`).
    // pub fn sub(&self) -> &str {
    //     &self.sub.to_string()
    // }

    // /// The workspace id encoded in the `ws` claim.
    // pub fn ws(&self) -> &str {
    //     &self.ws.to_string()
    // }

    // /// The membership id encoded in the `mem` claim.
    // pub fn mem(&self) -> &str {
    //     &self.mem.to_string()
    // }

    // pub fn mem_id(&self) -> Result<Uuid, CoreError> {
    //     Uuid::from_str(&self.mem)
    //         .map_err(|_| CoreError::Auth("invalid token: membership claim".into()))
    // }
    // pub fn acc_id(&self) -> Result<Uuid, CoreError> {
    //     Uuid::from_str(&self.sub)
    //         .map_err(|_| CoreError::Auth("invalid token: subject claim".into()))
    // }

    // pub fn ws_id(&self) -> Result<Uuid, CoreError> {
    //     Uuid::from_str(&self.ws).map_err(|_| CoreError::Auth("invalid token: workspace id".into()))
    // }

    /// The token type (`Auth`, `PasswordReset`, `Refresh`, `AccountConfirm`).
    pub fn token_type(&self) -> TokenType {
        self.ty
    }

    /// The session id encoded in the `sid` claim, if present.
    pub fn sid(&self) -> Option<Uuid> {
        self.sid
    }

    /// The unique JWT id (`jti`) claim, if present.
    pub fn jti(&self) -> Option<Uuid> {
        self.jti
    }

    /// Validates that the token is of the expected type.
    pub fn validate_type(&self, expected: TokenType) -> CoreResult<&Self> {
        if self.ty != expected {
            return Err(CoreError::Auth("invalid token type".to_string()));
        }
        Ok(self)
    }

    /// Validates that the token has not expired.
    pub fn validate_not_expired(&self) -> CoreResult<&Self> {
        if self.is_expired() {
            return Err(CoreError::Auth("token expired".to_string()));
        }
        Ok(self)
    }

    /// Returns the session id, erroring when the token has none.
    pub fn require_sid(&self) -> CoreResult<Uuid> {
        self.sid
            .ok_or_else(|| CoreError::Auth("invalid token".to_string()))
    }

    /// Returns the unique JWT id, erroring when the token has none.
    pub fn require_jti(&self) -> CoreResult<Uuid> {
        self.jti
            .ok_or_else(|| CoreError::Auth("invalid token".to_string()))
    }

    /// Validates a refresh token and returns the claims needed to issue a new session.
    pub fn validate_refresh(&self) -> CoreResult<RefreshClaims> {
        self.validate_type(TokenType::Refresh)?;
        let sid = self.require_sid()?;
        let jti = self.require_jti()?;
        let Self { sub, ws, mem, .. } = self;
        // let sub = self.acc_id()?;
        // let ws = self.ws_id()?;
        // let mem = self.mem_id()?;
        Ok(RefreshClaims {
            sub: sub.clone(),
            ws: ws.clone(),
            mem: mem.clone(),
            sid,
            jti,
        })
    }

    /// Validates an account-confirmation token and returns the account id.
    pub fn validate_account_confirm(&self) -> CoreResult<Uuid> {
        self.validate_type(TokenType::AccountConfirm)?;
        self.validate_not_expired()?;
        Ok(self.sub)
    }

    /// Validates a password-reset token and returns the account id.
    pub fn validate_password_reset(&self) -> CoreResult<Uuid> {
        self.validate_type(TokenType::PasswordReset)?;
        self.validate_not_expired()?;
        Ok(self.sub)
    }
}

/// Claims extracted from a validated refresh token.
#[derive(Debug, Clone)]
pub struct RefreshClaims {
    pub sub: Uuid, // Account ID
    pub ws: Uuid,  // Workspace ID
    pub mem: Uuid, // Membership ID
    pub sid: Uuid,
    pub jti: Uuid,
}
