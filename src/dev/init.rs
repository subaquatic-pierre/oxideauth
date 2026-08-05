use std::{
    env::current_dir,
    path::{Path, PathBuf},
};

use crate::{
    app::{new_app_data, AppEnv},
    cache::redis::RedisChx,
    store::{dbx::PgDbx, init::new_db_pool},
};
use anyhow::Result;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::OnceCell;
use tracing::info;

use crate::{
    app::AppState,
    dev::db::{init_dev_db, init_test_db},
    store::{manager::StoreManager, PgPool},
};

pub async fn init_dev(db_pool: &PgPool) {
    info!("{:<12} - init_dev()", "FOR-DEV-ONLY");
    init_dev_db(db_pool).await;
}

use std::sync::Once;
use tracing_subscriber::{fmt, EnvFilter};

static INIT_TRACING: Once = Once::new();

pub fn init_tracing_for_tests() {
    INIT_TRACING.call_once(|| {
        // Respect RUST_LOG if present, otherwise default to debug
        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));

        // swallow error if already initialized (useful when running many tests)
        let _ = fmt().with_env_filter(env_filter).try_init();
    });
}

pub async fn init_test<'a>() -> &'a AppState<PgDbx, RedisChx> {
    static INIT: OnceCell<AppState<PgDbx, RedisChx>> = OnceCell::const_new();

    let ds = INIT
        .get_or_init(|| async {
            info!("{:<12} - init_test()", "FOR-DEV-ONLY");

            let app = new_app_data(AppEnv::Test).await;
            init_test_db(&app.dbx.pool()).await;

            // init_tracing_for_tests();

            app
        })
        .await;

    ds
}
