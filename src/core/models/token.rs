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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::time::now_utc;
    use time::Duration;

    fn make_claims(ty: TokenType) -> TokenClaims {
        TokenClaims {
            sub: Uuid::new_v4(),
            ws: Uuid::new_v4(),
            mem: Uuid::new_v4(),
            iss: "oxideauth.app".to_string(),
            aud: "oxideauth.api".to_string(),
            exp: usize::MAX,
            iat: 0,
            ty,
            mem_ver: 3,
            acc_ver: 4,
            sid: Some(Uuid::new_v4()),
            jti: Some(Uuid::new_v4()),
        }
    }

    #[test]
    fn test_token_claims_new() {
        let sub = Uuid::new_v4();
        let ws = Uuid::new_v4();
        let mem = Uuid::new_v4();
        let sid = Uuid::new_v4();
        let jti = Uuid::new_v4();
        let exp = now_utc() + Duration::hours(2);

        let claims = TokenClaims::new(
            sub, ws, mem, exp, TokenType::Auth, 5, 6, Some(sid), Some(jti),
        );

        assert_eq!(claims.sub, sub);
        assert_eq!(claims.ws, ws);
        assert_eq!(claims.mem, mem);
        assert_eq!(claims.iss, "oxideauth.app");
        assert_eq!(claims.aud, "oxideauth.api");
        assert_eq!(claims.exp, exp.unix_timestamp() as usize);
        assert_eq!(claims.ty, TokenType::Auth);
        assert_eq!(claims.mem_ver, 5);
        assert_eq!(claims.acc_ver, 6);
        assert_eq!(claims.sid, Some(sid));
        assert_eq!(claims.jti, Some(jti));
        // iat is set at construction time; must be in the past or now
        assert!(claims.iat <= now_utc().unix_timestamp() as usize);
    }

    #[test]
    fn test_token_claims_new_without_session_ids() {
        let claims = TokenClaims::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now_utc() + Duration::minutes(5),
            TokenType::AccountConfirm,
            0,
            0,
            None,
            None,
        );
        assert!(claims.sid.is_none());
        assert!(claims.jti.is_none());
        assert_eq!(claims.ty, TokenType::AccountConfirm);
    }

    #[test]
    fn test_token_claims_is_expired() {
        let mut expired = make_claims(TokenType::Auth);
        expired.exp = 0;
        assert!(expired.is_expired());

        let mut valid = make_claims(TokenType::Auth);
        valid.exp = now_utc().unix_timestamp() as usize + 3600;
        assert!(!valid.is_expired());
    }

    #[test]
    fn test_token_claims_accessors() {
        let claims = make_claims(TokenType::PasswordReset);
        assert_eq!(claims.token_type(), TokenType::PasswordReset);
        assert_eq!(claims.sid(), claims.sid);
        assert_eq!(claims.jti(), claims.jti);
    }

    #[test]
    fn test_token_validate_type() {
        let claims = make_claims(TokenType::Refresh);
        assert!(claims.validate_type(TokenType::Refresh).is_ok());
        assert!(claims.validate_type(TokenType::Auth).is_err());
    }

    #[test]
    fn test_token_validate_not_expired() {
        let claims = make_claims(TokenType::Auth);
        assert!(claims.validate_not_expired().is_ok());

        let mut expired = make_claims(TokenType::Auth);
        expired.exp = 0;
        assert!(expired.validate_not_expired().is_err());
    }

    #[test]
    fn test_token_require_sid_and_jti() {
        let claims = make_claims(TokenType::Auth);
        assert_eq!(claims.require_sid().unwrap(), claims.sid.unwrap());
        assert_eq!(claims.require_jti().unwrap(), claims.jti.unwrap());

        let mut no_sid = make_claims(TokenType::Auth);
        no_sid.sid = None;
        assert!(no_sid.require_sid().is_err());

        let mut no_jti = make_claims(TokenType::Auth);
        no_jti.jti = None;
        assert!(no_jti.require_jti().is_err());
    }

    #[test]
    fn test_token_validate_refresh() {
        let claims = make_claims(TokenType::Refresh);
        let refresh = claims
            .validate_refresh()
            .expect("refresh token should validate");
        assert_eq!(refresh.sub, claims.sub);
        assert_eq!(refresh.ws, claims.ws);
        assert_eq!(refresh.mem, claims.mem);
        assert_eq!(refresh.sid, claims.sid.unwrap());
        assert_eq!(refresh.jti, claims.jti.unwrap());

        // Wrong token type
        let auth_claims = make_claims(TokenType::Auth);
        assert!(auth_claims.validate_refresh().is_err());

        // Missing sid
        let mut no_sid = make_claims(TokenType::Refresh);
        no_sid.sid = None;
        assert!(no_sid.validate_refresh().is_err());

        // Missing jti
        let mut no_jti = make_claims(TokenType::Refresh);
        no_jti.jti = None;
        assert!(no_jti.validate_refresh().is_err());
    }

    #[test]
    fn test_token_validate_account_confirm() {
        let claims = make_claims(TokenType::AccountConfirm);
        assert_eq!(claims.validate_account_confirm().unwrap(), claims.sub);

        let wrong = make_claims(TokenType::Auth);
        assert!(wrong.validate_account_confirm().is_err());

        let mut expired = make_claims(TokenType::AccountConfirm);
        expired.exp = 0;
        assert!(expired.validate_account_confirm().is_err());
    }

    #[test]
    fn test_token_validate_password_reset() {
        let claims = make_claims(TokenType::PasswordReset);
        assert_eq!(claims.validate_password_reset().unwrap(), claims.sub);

        let wrong = make_claims(TokenType::Refresh);
        assert!(wrong.validate_password_reset().is_err());

        let mut expired = make_claims(TokenType::PasswordReset);
        expired.exp = 0;
        assert!(expired.validate_password_reset().is_err());
    }

    #[test]
    fn test_token_claims_serde_round_trip() {
        let claims = make_claims(TokenType::Auth);
        let json = serde_json::to_string(&claims).expect("serialize");
        let back: TokenClaims = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, claims);
    }

    #[test]
    fn test_token_type_serde_round_trip() {
        for ty in [
            TokenType::Auth,
            TokenType::PasswordReset,
            TokenType::Refresh,
            TokenType::AccountConfirm,
        ] {
            let json = serde_json::to_string(&ty).unwrap();
            let back: TokenType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, ty);
        }
    }
}
