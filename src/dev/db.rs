use sqlx::migrate::Migrator;
use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use sqlx::{Executor, postgres::PgPoolOptions};
use tokio::sync::OnceCell;
use tracing::info;

use crate::{
    app::AppState,
    cache::redis::RedisChx,
    dev::{
        config::{PROJECT_ROOT, SQL_DIR},
        seed,
    },
    store::{dbx::PgDbx, init::PgPool, manager::StoreManager},
};

static INIT: OnceCell<()> = OnceCell::const_new();

pub async fn reset_db(pool: &PgPool) -> Result<()> {
    pool.execute("DROP SCHEMA public CASCADE").await?;
    pool.execute("CREATE SCHEMA public").await?;

    Ok(())
}

pub async fn run_migrations(pool: &PgPool, migration_env: &str) -> Result<()> {
    let path = get_sql_dir().join("migrations").join(migration_env);
    let migrator = Migrator::new(path).await?;
    migrator.run(pool).await?;

    Ok(())
}

pub async fn load_all_fixtures(app: &AppState<PgDbx, RedisChx>) -> Result<()> {
    seed::seed_all::<PgDbx, RedisChx>(app).await?;
    Ok(())
}

pub async fn init_dev_db(app: &AppState<PgDbx, RedisChx>) {
    reset_db(app.dbx.pool()).await.unwrap();
    run_migrations(app.dbx.pool(), "dev").await.unwrap();
    load_all_fixtures(app).await.unwrap();
}

pub async fn init_test_db(app: &AppState<PgDbx, RedisChx>) {
    reset_db(app.dbx.pool()).await.unwrap();
    run_migrations(app.dbx.pool(), "dev").await.unwrap();
    load_all_fixtures(app).await.unwrap();
}

pub fn get_sql_dir() -> PathBuf {
    let base_dir = PathBuf::from(PROJECT_ROOT);
    let sql_dir = base_dir.join(SQL_DIR);
    sql_dir
}

// pub async fn mock_store_manager() -> (StoreManager, PgPool) {
//     // 1. Create the mock pool from sqlx-mock.
//     let mock_pool = TestPostgres::default().get_pool().await;

//     // 2. Your StoreManager::new() function takes a `PgPool`.
//     //    `sqlx_mock::MockPool` can be cloned into a `PgPool`,
//     //    so we can pass it directly without any code changes!
//     let store_manager = StoreManager::new(mock_pool.clone());

//     // 3. Return both so the test can use them.
//     (store_manager, mock_pool)
// }

#[cfg(feature = "integration")]
#[cfg(test)]
mod tests {
    use crate::store::init::new_db_pool;

    use super::*;
    use crate::{
        app::{AppEnv, new_app_data},
        config::Config,
    };
    use anyhow::{Context, Result};
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_reset_db() -> Result<()> {
        let config = Config::test_config();
        let db = new_db_pool(&config.database_url, 1).await;

        reset_db(&db).await?;
        Ok(())
    }

    // #[tokio::test]
    // #[serial]
    // async fn test_run_migrations() -> Result<()> {
    //     let config = Config::test_config();
    //     let db = new_db_pool(&config.database_url, 1).await;

    //     reset_db(&db).await?;
    //     run_migrations(&db, "test").await?;

    //     Ok(())
    // }

    // #[tokio::test]
    // #[serial]
    // async fn test_load_fixture() -> Result<()> {
    //     let config = Config::test_config();
    //     let db = new_db_pool(&config.database_url, 1).await;

    //     init_test_db(&db).await;
    //     load_fixture(&db, "services.sql").await?;

    //     Ok(())
    // }

    // #[tokio::test]
    // #[serial]
    // async fn test_load_all_fixtures() -> Result<()> {
    //     let config = Config::test_config();
    //     let db = new_db_pool(&config.database_url, 1).await;

    //     init_test_db(&db).await;
    //     load_all_fixtures(&db).await?;

    //     Ok(())
    // }

    #[tokio::test]
    #[serial]
    async fn test_init_test_db() -> Result<()> {
        let app = new_app_data(AppEnv::Test).await;

        init_test_db(&app).await;

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_init_dev_db() -> Result<()> {
        let app = new_app_data(AppEnv::Test).await;

        init_dev_db(&app).await;

        Ok(())
    }
}
