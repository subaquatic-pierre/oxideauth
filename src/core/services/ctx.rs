use std::{collections::HashMap, sync::Arc};

use axum::{extract::Request, http::HeaderMap};
use tracing::info;

use crate::{
    cache::traits::CacheExecutor,
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        services::{account::AccountService, factory::ServiceFactory, token::TokenService},
    },
    store::{
        dbx::PgDbx, manager::StoreManager, stores::token::TokenStore, traits::dbx::DbExecutor,
    },
};

pub struct CtxConfig {}

pub struct CtxService<D, C>
where
    D: DbExecutor,
    C: CacheExecutor,
{
    svc_factory: Arc<ServiceFactory<D, C>>,
}

impl<D, C> CtxService<D, C>
where
    D: DbExecutor,
    C: CacheExecutor,
{
    pub fn new(svc_factory: Arc<ServiceFactory<D, C>>, config: CtxConfig) -> Self {
        Self { svc_factory }
    }

    pub async fn resolve_ctx(&self, headers: &HeaderMap) -> CoreResult<CoreCtx> {
        let token = match TokenService::<D, C>::token_str_from_headers(&headers) {
            Some(t) => {
                let token_svc = self.svc_factory.token();
                // decode token, will cause method to error if token signature or deserialization of claims fails, this means that request will return UNAUTHORIZED response. this is preferred behavior because token is on header. this means that request is malicious or has been tampered with, in which case return early
                let token = token_svc.decode_token_str(t)?;

                // if token exists and is in blacklist return unauthorized response
                if token_svc.is_blacklisted(&token) {
                    return Err(CoreError::Auth("token blacklisted".to_string()));
                }

                Some(token)
            }
            None => None,
        };

        info!("TOKEN {token:?} - resolve_ctx");

        // TODO: return correct built context
        CoreCtx::new_test()
    }
}
