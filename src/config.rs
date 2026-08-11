use std::{env::var, sync::Arc};

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: usize,
    pub app_env: String,

    pub client_origin: String,
    pub database_url: String,
    pub redis_url: String,
    pub jwt_secret: String,
    pub access_token_max_age: u64,
    pub refresh_token_max_age: u64,

    pub google_oauth_client_id: String,
    pub google_oauth_client_secret: String,
    pub google_oauth_redirect_url: String,

    pub aws_region: String,

    pub aws_ses_from: String,
    pub aws_ses_host: String,

    pub aws_ses_access_key: String,
    pub aws_ses_secret_key: String,

    pub aws_s3_access_key: String,
    pub aws_s3_secret_key: String,
    pub email_dry_mode: bool,

    pub owner_email: String,
    pub owner_password: String,
    pub owner_name: String,
}

impl Config {
    pub fn from_env() -> Self {
        let database_url = var("DATABASE_URL").expect("DATABASE_URL must be set");
        let redis_url = var("REDIS_URL").expect("REDIS_URL must be set");
        let host = var("HOST").unwrap_or("http://localhost".to_string());
        let port = var("PORT")
            .unwrap_or("8000".to_string())
            .parse::<usize>()
            .expect("Unable to parse PORT from .env, value must be valid number");

        let client_origin = var("CLIENT_ORIGIN").expect("CLIENT_ORIGIN must be set");
        let jwt_secret = var("JWT_SECRET").expect("JWT_SECRET must be set");
        let access_token_max_age = var("ACCESS_TOKEN_MAXAGE")
            .unwrap_or("900".to_string()) // 15 minutes default
            .parse::<u64>()
            .unwrap();
        let refresh_token_max_age = var("REFRESH_TOKEN_MAXAGE")
            .unwrap_or("604800".to_string()) // 7 days default
            .parse::<u64>()
            .unwrap();

        let google_oauth_client_id =
            var("GOOGLE_OAUTH_CLIENT_ID").expect("GOOGLE_OAUTH_CLIENT_ID must be set");
        let google_oauth_client_secret =
            var("GOOGLE_OAUTH_CLIENT_SECRET").expect("GOOGLE_OAUTH_CLIENT_SECRET must be set");
        let google_oauth_redirect_url =
            var("GOOGLE_OAUTH_REDIRECT_URL").expect("GOOGLE_OAUTH_REDIRECT_URL must be set");

        let aws_ses_host = var("AWS_SES_HOST").expect("AWS_SES_HOST must be set in .env");
        let aws_ses_access_key =
            var("AWS_SES_ACCESS_KEY").expect("AWS_SES_ACCESS_KEY credentials must be set in .env");
        let aws_ses_secret_key =
            var("AWS_SES_SECRET_KEY").expect("AWS_SES_SECRET_KEY credentials must be set in .env");

        let aws_s3_access_key =
            var("AWS_S3_ACCESS_KEY").expect("AWS_SES_ACCESS_KEY credentials must be set in .env");
        let aws_s3_secret_key =
            var("AWS_S3_SECRET_KEY").expect("AWS_SES_SECRET_KEY credentials must be set in .env");

        let aws_ses_from =
            var("AWS_SES_FROM").expect("AWS_SES_FROM credentials must be set in .env");
        let aws_region = var("AWS_REGION").expect("AWS_REGION credentials must be set in .env");

        let app_env = var("APP_ENV").expect("APP_ENV must be set in .env");

        let owner_email =
            var("OWNER_EMAIL").unwrap_or_else(|_| "owner@system.local".to_string());
        let owner_password = var("OWNER_PASSWORD").unwrap_or_else(|_| "ownerpass".to_string());
        let owner_name = var("OWNER_NAME").unwrap_or_else(|_| "Owner Account".to_string());

        Config {
            database_url,
            redis_url,
            jwt_secret,
            client_origin,
            access_token_max_age,
            refresh_token_max_age,
            google_oauth_client_id,
            google_oauth_client_secret,
            google_oauth_redirect_url,
            host,
            port,
            aws_ses_host,
            aws_ses_access_key,
            aws_ses_secret_key,
            aws_ses_from,
            aws_region,
            aws_s3_access_key,
            aws_s3_secret_key,
            email_dry_mode: false,
            app_env,
            owner_email,
            owner_password,
            owner_name,
        }
    }

    pub fn mock_config() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            app_env: "mock".to_string(),
            port: 8000,
            client_origin: "http://localhost:5000".to_string(),
            database_url: "postgres://user:password@localhost/test_db".to_string(),
            redis_url: "redis://127.0.0.1:6379".to_string(),
            jwt_secret: "supersecretkey".to_string(),
            access_token_max_age: 900,
            refresh_token_max_age: 604800,
            google_oauth_client_id: "mock-client-id".to_string(),
            google_oauth_client_secret: "mock-client-secret".to_string(),
            google_oauth_redirect_url: "http://localhost:8000/oauth/google/callback".to_string(),
            aws_region: "us-east-1".to_string(),
            aws_ses_from: "no-reply@example.com".to_string(),
            aws_ses_host: "email-smtp.us-east-1.amazonaws.com".to_string(),
            aws_ses_access_key: "mock-access-key".to_string(),
            aws_ses_secret_key: "mock-secret-key".to_string(),
            aws_s3_access_key: "mock-s3-access-key".to_string(),
            aws_s3_secret_key: "mock-s3-secret-key".to_string(),
            email_dry_mode: true,
            owner_email: "owner@system.local".to_string(),
            owner_password: "ownerpass".to_string(),
            owner_name: "Owner Account".to_string(),
        }
    }

    pub fn test_config() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            app_env: "test".to_string(),
            port: 8000,
            client_origin: "http://localhost:5000".to_string(),
            database_url: "postgres://test_user:password@localhost:5432/test_db".to_string(),
            redis_url: "redis://127.0.0.1:6379".to_string(),
            // redis_url: "redis://oxideauth-redis:6379".to_string(),
            jwt_secret: "supersecretkey".to_string(),
            access_token_max_age: 900,
            refresh_token_max_age: 604800,
            google_oauth_client_id: "mock-client-id".to_string(),
            google_oauth_client_secret: "mock-client-secret".to_string(),
            google_oauth_redirect_url: "http://localhost:8000/oauth/google/callback".to_string(),
            aws_region: "us-east-1".to_string(),
            aws_ses_from: "no-reply@example.com".to_string(),
            aws_ses_host: "email-smtp.us-east-1.amazonaws.com".to_string(),
            aws_ses_access_key: "mock-access-key".to_string(),
            aws_ses_secret_key: "mock-secret-key".to_string(),
            aws_s3_access_key: "mock-s3-access-key".to_string(),
            aws_s3_secret_key: "mock-s3-secret-key".to_string(),
            email_dry_mode: true,
            owner_email: "owner@system.local".to_string(),
            owner_password: "ownerpass".to_string(),
            owner_name: "Owner Account".to_string(),
        }
    }

    // pub fn dev_config() -> Self {
    //     Self {
    //         host: "127.0.0.1".to_string(),
    //         app_env: "dev".to_string(),
    //         port: 8000,
    //         client_origin: "http://localhost:5000".to_string(),
    //         database_url: "postgres://test_user:password@localhost:5432/dev_db".to_string(),
    //         // TODO: update redis url
    //         redis_url: "postgres://test_user:password@localhost:5432/test_db".to_string(),
    //         jwt_secret: "supersecretkey".to_string(),
    //         jwt_max_age: 3600,
    //         google_oauth_client_id: "mock-client-id".to_string(),
    //         google_oauth_client_secret: "mock-client-secret".to_string(),
    //         google_oauth_redirect_url: "http://localhost:8000/oauth/google/callback".to_string(),
    //         aws_region: "us-east-1".to_string(),
    //         aws_ses_from: "no-reply@example.com".to_string(),
    //         aws_ses_host: "email-smtp.us-east-1.amazonaws.com".to_string(),
    //         aws_ses_access_key: "mock-access-key".to_string(),
    //         aws_ses_secret_key: "mock-secret-key".to_string(),
    //         aws_s3_access_key: "mock-s3-access-key".to_string(),
    //         aws_s3_secret_key: "mock-s3-secret-key".to_string(),
    //         email_dry_mode: true,
    //     }
    // }
}
