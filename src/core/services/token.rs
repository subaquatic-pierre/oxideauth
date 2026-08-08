use std::time::Duration;
use tracing::debug;

use axum::http::{HeaderMap, header::AUTHORIZATION};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};

use crate::{
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::token::TokenClaims,
        services::workspace::WorkspaceService,
    },
    store::traits::dbx::DbExecutor,
    utils::time::now_utc,
};

pub struct TokenServiceConfig {
    jwt_secret: String,
    pub encoding_key: EncodingKey,
    pub decoding_key: DecodingKey,
    access_token_max_age: u64,
    refresh_token_max_age: u64,
    algo: Validation,
}

impl TokenServiceConfig {
    pub fn new(jwt_secret: String, access_token_max_age: u64, refresh_token_max_age: u64) -> Self {
        let jwt_secret_bytes = jwt_secret.as_bytes();
        let encoding_key = EncodingKey::from_secret(&jwt_secret_bytes);
        let decoding_key = DecodingKey::from_secret(&jwt_secret_bytes);
        let algo = Validation::new(Algorithm::HS256);
        Self {
            jwt_secret,
            access_token_max_age,
            refresh_token_max_age,
            encoding_key,
            decoding_key,
            algo,
        }
    }

    pub fn access_token_max_age(&self) -> u64 {
        self.access_token_max_age
    }

    pub fn refresh_token_max_age(&self) -> u64 {
        self.refresh_token_max_age
    }
}

pub struct TokenService<D: DbExecutor> {
    ws_svc: WorkspaceService<D>,
    config: TokenServiceConfig,
}

impl<D: DbExecutor> TokenService<D> {
    pub fn new(ws_svc: WorkspaceService<D>, config: TokenServiceConfig) -> Self {
        Self { config, ws_svc }
    }

    pub fn decode_token_str(&self, token_str: &str) -> CoreResult<TokenClaims> {
        debug!(
            "token_str in TokenService.decode_token_str: {:?}",
            token_str
        );

        let data = decode::<TokenClaims>(&token_str, &self.config.decoding_key, &self.config.algo)?;

        debug!(
            "TokenClaims in TokenService.decode_token_str: {:?}",
            data.claims
        );

        Ok(data.claims)
    }

    pub fn token_header(&self) -> Header {
        Header::default()
    }

    pub fn encode_token_claims(&self, claims: &TokenClaims) -> CoreResult<String> {
        let token = encode(&Header::default(), &claims, &self.config.encoding_key)?;

        Ok(token)
    }

    pub fn gen_access_token_exp_time(&self) -> usize {
        let now = now_utc();
        let expire_duration = Duration::from_secs(self.config.access_token_max_age);
        let future_time = now + expire_duration;
        future_time.unix_timestamp_nanos() as usize
    }

    pub fn gen_refresh_token_exp_time(&self) -> usize {
        let now = now_utc();
        let expire_duration = Duration::from_secs(self.config.refresh_token_max_age);
        let future_time = now + expire_duration;
        future_time.unix_timestamp_nanos() as usize
    }

    pub fn is_token_exp(token: &TokenClaims) -> bool {
        token.is_expired()
    }

    pub fn token_str_from_headers<'b>(headers: &'b HeaderMap) -> Option<&'b str> {
        let auth_header = headers.get(AUTHORIZATION).and_then(|h| h.to_str().ok());

        let token = match auth_header {
            Some(str) => {
                let mut iter = str.split(" ").into_iter();

                let start = iter.next();
                let token = iter.next();
                token
            }
            None => None,
        };

        token
    }
}
