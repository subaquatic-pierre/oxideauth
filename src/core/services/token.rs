use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

use axum::http::{HeaderMap, header::AUTHORIZATION};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};

use crate::{
    cache::traits::CacheExecutor,
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
    access_token_max_age: i64,
    refresh_token_max_age: i64,
    algo: Validation,
}

impl TokenServiceConfig {
    pub fn new(jwt_secret: String, access_token_max_age: i64, refresh_token_max_age: i64) -> Self {
        let jwt_secret_bytes = jwt_secret.as_bytes();
        let encoding_key = EncodingKey::from_secret(&jwt_secret_bytes);
        let decoding_key = DecodingKey::from_secret(&jwt_secret_bytes);
        let mut algo = Validation::new(Algorithm::HS256);
        algo.set_audience(&["oxideauth.api"]);
        algo.set_issuer(&["oxideauth.app"]);
        Self {
            jwt_secret,
            access_token_max_age,
            refresh_token_max_age,
            encoding_key,
            decoding_key,
            algo,
        }
    }

    pub fn access_token_max_age(&self) -> i64 {
        self.access_token_max_age
    }

    pub fn refresh_token_max_age(&self) -> i64 {
        self.refresh_token_max_age
    }
}

pub struct TokenService<D: DbExecutor, C: CacheExecutor> {
    ws_svc: Arc<WorkspaceService<D, C>>,
    config: TokenServiceConfig,
}

impl<D: DbExecutor, C: CacheExecutor> TokenService<D, C> {
    pub fn new(ws_svc: Arc<WorkspaceService<D, C>>, config: TokenServiceConfig) -> Self {
        Self { config, ws_svc }
    }

    pub fn decode_token_str(&self, token_str: &str) -> CoreResult<TokenClaims> {
        // debug!(
        //     "token_str in TokenService.decode_token_str: {:?}",
        //     token_str
        // );

        let data = decode::<TokenClaims>(&token_str, &self.config.decoding_key, &self.config.algo)?;

        // debug!(
        //     "TokenClaims in TokenService.decode_token_str: {:?}",
        //     data.claims
        // );

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
        let expire_duration = Duration::from_secs(self.config.access_token_max_age as u64);
        let future_time = now + expire_duration;
        future_time.unix_timestamp_nanos() as usize
    }

    pub fn gen_refresh_token_exp_time(&self) -> usize {
        let now = now_utc();
        let expire_duration = Duration::from_secs(self.config.refresh_token_max_age as u64);
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        cache::{manager::CacheManager, mock::MockChx},
        config::Config,
        core::{models::token::TokenType, services::registry::ServiceRegistry},
        store::{dbx::MockDbx, manager::StoreManager},
        utils::time::now_utc,
    };
    use axum::http::HeaderMap;
    use serial_test::serial;
    use time::Duration as TimeDuration;
    use uuid::Uuid;

    /// Builds a `TokenService` backed by an in-memory `MockDbx` + `MockChx`
    /// (no real DB or Redis needed for token encode/decode).
    fn test_svc() -> Arc<TokenService<MockDbx, MockChx>> {
        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(MockDbx::new())));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        svc_reg.token.clone()
    }

    fn auth_claims() -> TokenClaims {
        TokenClaims::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now_utc() + TimeDuration::hours(1),
            TokenType::Auth,
            0,
            0,
            Some(Uuid::new_v4()),
            Some(Uuid::new_v4()),
        )
    }

    #[tokio::test]
    #[serial]
    async fn test_token_roundtrip() -> CoreResult<()> {
        // -- Setup
        let svc = test_svc();
        let claims = auth_claims();

        // -- Execute
        let encoded = svc.encode_token_claims(&claims)?;
        let decoded = svc.decode_token_str(&encoded)?;

        // -- Assert
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.ws, claims.ws);
        assert_eq!(decoded.mem, claims.mem);
        assert_eq!(decoded.ty, claims.ty);
        assert_eq!(decoded.mem_ver, claims.mem_ver);
        assert_eq!(decoded.acc_ver, claims.acc_ver);
        assert_eq!(decoded.sid, claims.sid);
        assert_eq!(decoded.jti, claims.jti);
        assert!(!decoded.is_expired());

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_decode_token_invalid() -> CoreResult<()> {
        // -- Setup
        let svc = test_svc();

        // -- Execute
        let res = svc.decode_token_str("not-a-valid-jwt");

        // -- Assert
        assert!(res.is_err(), "decoding garbage should fail");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_token_str_from_headers() -> CoreResult<()> {
        // -- Setup
        let token = "jwt-token-value";

        // -- Execute: valid Bearer header
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, format!("Bearer {token}").parse().unwrap());
        let extracted = TokenService::<MockDbx, MockChx>::token_str_from_headers(&headers);

        // -- Assert
        assert_eq!(extracted, Some(token));

        // -- Execute: no authorization header
        let headers = HeaderMap::new();
        assert_eq!(
            TokenService::<MockDbx, MockChx>::token_str_from_headers(&headers),
            None
        );

        // -- Execute: bearer without a token part
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "Bearer".parse().unwrap());
        assert_eq!(
            TokenService::<MockDbx, MockChx>::token_str_from_headers(&headers),
            None
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_gen_token_exp_times() -> CoreResult<()> {
        // -- Setup
        let config = Config::test_config();
        let svc = test_svc();
        let now = now_utc().unix_timestamp_nanos() as usize;

        // -- Execute
        let access = svc.gen_access_token_exp_time();
        let refresh = svc.gen_refresh_token_exp_time();

        // -- Assert
        assert!(access > now, "access exp must be in the future");
        assert!(refresh > now, "refresh exp must be in the future");
        assert!(
            refresh >= access,
            "refresh lifetime should be >= access lifetime"
        );
        assert_eq!(
            svc.config.access_token_max_age(),
            config.access_token_max_age
        );
        assert_eq!(
            svc.config.refresh_token_max_age(),
            config.refresh_token_max_age
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_is_token_exp() -> CoreResult<()> {
        // -- Setup: expired + valid claims
        let expired = TokenClaims::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now_utc() - TimeDuration::hours(1),
            TokenType::Auth,
            0,
            0,
            None,
            None,
        );
        let valid = auth_claims();

        // -- Execute / -- Assert
        assert!(TokenService::<MockDbx, MockChx>::is_token_exp(&expired));
        assert!(!TokenService::<MockDbx, MockChx>::is_token_exp(&valid));

        Ok(())
    }
}
