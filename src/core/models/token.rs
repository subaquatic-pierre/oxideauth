use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::utils::time::now_utc;

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
pub struct TokenClaims {
    sub: String, // Account ID
    ws: String,  // Workspace ID
    mem: String, // Membership ID
    iss: String,
    aud: String,
    pub exp: usize,
    pub iat: usize,
    ty: TokenType,
    pub mem_ver: u64,      // Membership token version
    pub acc_ver: u64,      // Account token version
    pub sid_ver: u64,      // Session version (default 0)
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
        sid_ver: u64,
        sid: Option<Uuid>,
        jti: Option<Uuid>,
    ) -> Self {
        Self {
            sub: sub.to_string(),
            ws: ws.to_string(),
            mem: mem.to_string(),
            iss: "oxideauth.app".to_string(),
            aud: "oxideauth.api".to_string(),
            iat: now_utc().unix_timestamp() as usize,
            exp: exp.unix_timestamp() as usize,
            ty,
            mem_ver,
            acc_ver,
            sid_ver,
            sid,
            jti,
        }
    }

    pub fn is_expired(&self) -> bool {
        let now = now_utc().unix_timestamp() as usize;
        if self.exp < now {
            true
        } else {
            false
        }
    }

    /// The account id encoded in the token subject (`sub`).
    pub fn sub(&self) -> &str {
        &self.sub
    }

    /// The workspace id encoded in the `ws` claim.
    pub fn ws(&self) -> &str {
        &self.ws
    }

    /// The membership id encoded in the `mem` claim.
    pub fn mem(&self) -> &str {
        &self.mem
    }

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
}
