use std::{env, sync::Arc};

use tracing::{debug, info};

use sqlx::Pool;

use crate::cache::manager::CacheManager;
use crate::cache::redis::RedisChx;
use crate::cache::traits::CacheExecutor;
use crate::core::ctx::{ContextFactory, CoreCtx};
use crate::core::error::CoreResult;
use crate::core::services::registry::ServiceRegistry;
use crate::core::services::token::TokenService;
use crate::dev::db::init_dev_db;
use crate::store::dbx::PgDbx;
use crate::store::manager::StoreManager;
use crate::store::traits::dbx::DbExecutor;
use crate::{
    config::Config,
    store::init::{PgPool, new_db_pool},
};

pub enum AppEnv {
    Development,
    Production,
    Test,
}

impl AppEnv {
    pub fn from_env() -> Self {
        let app_env = env::var("APP_ENV").expect("APP_ENV must be set in your .env file");

        match app_env.as_str() {
            "dev" => AppEnv::Development,
            "prod" => AppEnv::Production,
            "test" => AppEnv::Test,
            _ => panic!("incorrect environment value set for APP_ENV, must be 'prod' or 'dev"),
        }
    }
}

pub struct AppState<D, C>
where
    D: DbExecutor,
    C: CacheExecutor,
{
    pub config: Config,
    pub dbx: Arc<D>,
    pub chx: Arc<C>,
    pub sm: Arc<StoreManager<D>>,
    pub cm: Arc<CacheManager<C>>,
    pub svc_reg: Arc<ServiceRegistry<D, C>>,
}

impl<D: DbExecutor, C: CacheExecutor> AppState<D, C> {
    /// Creates a system-level `CoreCtx` authenticated as the system account.
    ///
    /// Used by unauthenticated flows (registration, login, password reset, OAuth)
    /// that need a properly scoped context without a pre-existing user session.
    /// Uses the cached system workspace and account UUIDs so audit fields
    /// carry a real, traceable identity.
    pub fn system_context(&self) -> CoreResult<CoreCtx> {
        self.svc_reg.ctx_factory.system()
    }
}

pub async fn new_app_data(app_env: AppEnv) -> AppState<PgDbx, RedisChx> {
    let (sm, cm, dbx, chx, config) = match app_env {
        AppEnv::Development => {
            let config = Config::from_env();
            let db: PgPool = new_db_pool(&config.database_url, 1).await;

            // TODO: add worker to app state, to manager all long running
            // background tasks, in the future may need to use
            // message bus, with dedicated worker services to handle scaling
            let dbx = Arc::new(PgDbx::new(db.clone()));
            let sm = Arc::new(StoreManager::new(dbx.clone()));

            let chx = Arc::new(RedisChx::new(&config.redis_url).await);
            let cm = Arc::new(CacheManager::new(chx.clone()));

            (sm, cm, dbx, chx, config)
        }
        AppEnv::Production => {
            let config = Config::from_env();
            let db: PgPool = new_db_pool(&config.database_url, 5).await;

            let dbx = Arc::new(PgDbx::new(db.clone()));
            let sm = Arc::new(StoreManager::new(dbx.clone()));

            debug!(
                "{:<12} - new_app_data()",
                "Application started in PRODUCTION mode"
            );

            let chx = Arc::new(RedisChx::new(&config.redis_url).await);
            let cm = Arc::new(CacheManager::new(chx.clone()));

            (sm, cm, dbx, chx, config)
        }
        _ => {
            let config = Config::test_config();
            let db: PgPool = new_db_pool(&config.database_url, 2).await;
            let dbx = Arc::new(PgDbx::new(db.clone()));
            let sm = Arc::new(StoreManager::new(dbx.clone()));

            debug!(
                "{:<12} - new_app_data()",
                "Application started in TEST mode"
            );

            let chx = Arc::new(RedisChx::new(&config.redis_url).await);
            let cm = Arc::new(CacheManager::new(chx.clone()));

            (sm, cm, dbx, chx, config)
        }
    };

    let svc_reg = Arc::new(ServiceRegistry::new(&config, sm.clone(), cm.clone()));

    // debug!("App Config config: {:?}", config);

    let app = AppState {
        dbx: dbx.clone(),
        config,
        chx,
        cm,
        sm,
        svc_reg,
    };

    // TODO: Ensure this is never run in production
    debug!("app.config: {:#?}", app.config);
    match app_env {
        AppEnv::Development => {
            if app.config.reset_db {
                init_dev_db(&app).await;

                debug!("Running init_dev_db, database is reset and seeded");
                // debug!("DEVELOPMENT mode config: {:#?}", app.config);
            }
        }
        _ => {

            // debug!("App Config config: {:?}", config);
        }
    }

    app
}

pub type App = Arc<AppState<PgDbx, RedisChx>>;
