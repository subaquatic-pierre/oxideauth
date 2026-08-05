use std::time::Duration;

use tokio::{task::JoinHandle, time::interval};
use tracing::info;

use crate::store::PgPool;

pub struct WorkerManager {}

impl WorkerManager {
    pub fn spawn_token_cleanup_worker(dbx: PgPool) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut timer = interval(Duration::from_secs(30));
            loop {
                timer.tick().await;

                let query = sqlx::query("DELETE FROM token WHERE expires_at < now()");

                match query.execute(&dbx).await {
                    Ok(res) => println!("Cleaned up {} expired tokens", res.rows_affected()),
                    Err(e) => eprintln!("Token cleanup error: {}", e),
                }

                info!("Running SQL to cleanup  expired tokens in 'token' table ...")
            }
        })
    }
}
