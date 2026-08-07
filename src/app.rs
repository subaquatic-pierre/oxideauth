use std::{env, sync::Arc};

use tracing::{debug, info};

use sqlx::Pool;

use crate::cache::manager::CacheManager;
use crate::cache::redis::RedisChx;
use crate::cache::traits::CacheExecutor;
use crate::core::services::factory::ServiceFactory;
use crate::core::services::token::TokenService;
use crate::dev::init::init_dev;
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
    pub svc_factory: Arc<ServiceFactory<D, C>>,
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

            debug!(
                "{:<12} - new_app_data()",
                "Application started in DEVELOPMENT mode",
            );

            init_dev(&dbx.pool()).await;

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
            let db: PgPool = new_db_pool(&config.database_url, 1).await;
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

    let svc_factory = Arc::new(ServiceFactory::new(sm.clone(), cm.clone()));

    debug!("App Config config: {:?}", config);

    AppState {
        dbx: dbx.clone(),
        config,
        chx,
        cm,
        sm,
        svc_factory,
    }
}

pub type App = Arc<AppState<PgDbx, RedisChx>>;
