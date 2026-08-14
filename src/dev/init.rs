use std::{
    env::current_dir,
    path::{Path, PathBuf},
};

use crate::{
    app::{AppEnv, new_app_data},
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
    store::{PgPool, manager::StoreManager},
};

use std::sync::Once;
use tracing_subscriber::{EnvFilter, fmt};

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

/// Migrate + seed the database exactly once, against a throwaway pool.
/// The seeded data lives in Postgres and is reused by every test.
async fn ensure_seeded() {
    static SEED: OnceCell<()> = OnceCell::const_new();
    SEED.get_or_init(|| async {
        info!("{:<12} - init_test() seeding", "FOR-DEV-ONLY");
        let seed_app = new_app_data(AppEnv::Test).await;
        init_test_db(&seed_app).await; // reset_db + migrations + fixtures
    })
    .await;
}

pub async fn init_test() -> AppState<PgDbx, RedisChx> {
    ensure_seeded().await;
    new_app_data(AppEnv::Test).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `init_tracing_for_tests` is the only DB-free helper in this module;
    /// `init_test()`/`ensure_seeded()` require a live Postgres/Redis, so they
    /// are intentionally not unit-tested here.
    #[test]
    fn test_init_tracing_for_tests_is_idempotent() {
        // Must not panic even when invoked multiple times.
        init_tracing_for_tests();
        init_tracing_for_tests();
    }
}
