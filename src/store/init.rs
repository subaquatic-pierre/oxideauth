use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};

// Type alias for DB
pub type PgPool = Pool<Postgres>;

pub async fn new_db_pool(database_url: &str, max_connections: u32) -> PgPool {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
        .expect("Failed to create pool")
}
