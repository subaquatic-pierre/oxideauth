use serde::{Deserialize, Serialize};
use std::str::FromStr;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    core::models::membership::Membership,
    store::entities::token::{TokenForCreate, TokenKind, TokenMeta},
    utils::time::{format_time, now_utc},
};

use modql::filter::OpValString;

use crate::{
    core::{
        error::CoreResult,
        models::{
            account::Account,
            audit::CoreAuditFields,
            list::{RequestFilterParams, RequestListOptions},
            workspace::Workspace,
        },
        traits::{
            filter::{OpValAccountId, OpValWorkspaceId},
            list::RequestListParams,
        },
    },
    store::entities::hash::Sha256Hash,
    store::entities::token::{TokenFilter as StoreTokenFilter, TokenRow},
};

pub type TokenFilter = StoreTokenFilter;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Token {
    pub id: Uuid,
    pub hash: Sha256Hash,

    pub kind: TokenKind,

    // Relations are optional in the blacklist context
    pub account_id: Uuid,
    pub workspace_id: Uuid,

    pub expires_at: OffsetDateTime,
    pub reason: Option<String>,

    pub audit: CoreAuditFields,
}

impl From<TokenRow> for Token {
    fn from(value: TokenRow) -> Self {
        Self {
            id: value.id.into(),
            hash: value.hash,
            kind: value.kind,
            account_id: value.account_id,
            workspace_id: value.workspace_id,
            expires_at: value.expires_at,
            reason: value.reason,
            audit: value.audit.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TokenDescribeParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct TokenDeleteParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct TokenCreateParams {
    pub hash: Sha256Hash,
    pub kind: TokenKind,
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub expires_at: OffsetDateTime,
    pub reason: Option<String>,
}

impl From<TokenCreateParams> for TokenForCreate {
    fn from(params: TokenCreateParams) -> Self {
        Self {
            hash: params.hash,
            kind: params.kind,
            account_id: params.account_id,
            workspace_id: params.workspace_id,
            expires_at: params.expires_at,
            reason: params.reason,
            // Defaulting system/store fields not present in simple CreateParams
            tags: Vec::new(),
            meta: TokenMeta::default(),
        }
    }
}

pub struct TokenListParams {
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<TokenFilter>>,
    pub options: Option<RequestListOptions>,
}

impl RequestListParams<TokenFilter> for TokenListParams {
    fn filter(&self) -> Option<RequestFilterParams<TokenFilter>> {
        self.filter.clone()
    }

    fn options(&self) -> Option<RequestListOptions> {
        self.options.clone()
    }
}

impl OpValWorkspaceId for TokenFilter {
    fn get_workspace_id_opval(&self) -> Option<&OpValString> {
        self.workspace_id
            .as_ref()
            .and_then(|op_vals| op_vals.0.first())
    }
}

impl OpValAccountId for TokenFilter {
    fn get_account_id_opval(&self) -> Option<&OpValString> {
        self.account_id
            .as_ref()
            .and_then(|op_vals| op_vals.0.first())
    }
}

impl Default for Token {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            hash: Sha256Hash::default(),
            kind: TokenKind::Auth,
            account_id: Uuid::default(),
            workspace_id: Uuid::default(),
            expires_at: OffsetDateTime::now_utc(),
            reason: None,
            audit: CoreAuditFields::default(),
        }
    }
}

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
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy, Deserialize)]
pub enum TokenType {
    Auth,
    PasswordReset,
    Refresh,
    AccountConfirm,
}

impl TokenClaims {
    pub fn new(sub: Uuid, ws: Uuid, mem: Uuid, exp: OffsetDateTime, ty: TokenType) -> Self {
        Self {
            sub: sub.to_string(),
            ws: ws.to_string(),
            mem: mem.to_string(),
            iss: "oxideauth.app".to_string(),
            aud: "oxideauth.api".to_string(),
            iat: now_utc().unix_timestamp() as usize,
            exp: exp.unix_timestamp() as usize,
            ty,
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
}
