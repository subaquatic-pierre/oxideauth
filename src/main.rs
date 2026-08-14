use std::sync::Arc;

use dotenv::dotenv;
use tracing::info;
use tracing_subscriber::EnvFilter;

use oxideauth::{
    app::{new_app_data, AppEnv},
    web::router::AppRouter,
};

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt()
        .without_time() // For early local development.
        .with_target(false)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let app_env = AppEnv::from_env();
    let app = new_app_data(app_env).await;

    // Define the address to run the server on.
    let bind_addr = format!("{}:{}", app.config.host, app.config.port);
    info!("Server listening at {bind_addr} ... ",);

    // Create a TCP listener and serve the application.
    let listener = tokio::net::TcpListener::bind(bind_addr).await.unwrap();

    let state = Arc::new(app);

    // Define the application's routes.
    let router = AppRouter::routes_with_state(state);

    axum::serve(listener, router).await.unwrap();
}
