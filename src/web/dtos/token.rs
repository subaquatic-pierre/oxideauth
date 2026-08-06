use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::models::{
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
    token::{
        Token, TokenDeleteParams, TokenDescribeParams, TokenFilter, TokenListParams,
    },
};
use crate::store::entities::token::TokenKind;

// --- TokenDescribeReq ---
#[derive(Deserialize)]
pub struct TokenDescribeReq {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

impl From<TokenDescribeReq> for TokenDescribeParams {
    fn from(value: TokenDescribeReq) -> Self {
        Self {
            id: value.id,
            workspace_id: value.workspace_id,
        }
    }
}

// --- TokenDescribeRes ---
// SECURITY: hash field is NOT serialized — raw JWT values are never exposed
#[derive(Serialize, Debug)]
pub struct TokenDescribeRes {
    pub id: Uuid,
    pub kind: TokenKind,
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    pub reason: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
}

impl From<Token> for TokenDescribeRes {
    fn from(t: Token) -> Self {
        Self {
            id: t.id,
            kind: t.kind,
            account_id: t.account_id,
            workspace_id: t.workspace_id,
            expires_at: t.expires_at,
            reason: t.reason,
            created_at: t.audit.created_at,
            updated_at: t.audit.updated_at,
        }
    }
}

// NOTE: No TokenCreateReq — create is excluded per spec
// NOTE: No TokenUpdateReq — update is excluded per spec

// --- TokenListReq ---
#[derive(Deserialize, Debug)]
pub struct TokenListReq {
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<TokenFilter>>,
    pub options: Option<RequestListOptions>,
}

impl From<TokenListReq> for TokenListParams {
    fn from(value: TokenListReq) -> Self {
        Self {
            workspace_id: value.workspace_id,
            filter: value.filter,
            options: value.options,
        }
    }
}

// --- TokenListRes ---
#[derive(Serialize, Debug)]
pub struct TokenListRes {
    pub tokens: Vec<TokenDescribeRes>,
    pub metadata: ListResponseMeta,
}

// --- TokenDeleteReq ---
#[derive(Deserialize)]
pub struct TokenDeleteReq {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

impl From<TokenDeleteReq> for TokenDeleteParams {
    fn from(value: TokenDeleteReq) -> Self {
        Self {
            id: value.id,
            workspace_id: value.workspace_id,
        }
    }
}

// --- TokenDeleteRes ---
#[derive(Serialize)]
pub struct TokenDeleteRes {
    pub id: Uuid,
}
