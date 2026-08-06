use std::{env, io, str::FromStr, sync::Arc};

use dotenv::dotenv;
use tracing::info;
use tracing_subscriber::EnvFilter;

use axum::{Router, routing::get};
use std::net::SocketAddr;

mod app;
mod cache;
mod config;
mod core;
mod dev;
mod macros;
mod store;
mod utils;
mod web;

use app::new_app_data;

use crate::{app::AppEnv, web::router::AppRouter};

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
    let addr = SocketAddr::from_str(&bind_addr);

    // Create a TCP listener and serve the application.
    let listener = tokio::net::TcpListener::bind(bind_addr).await.unwrap();

    let state = Arc::new(app);

    // Define the application's routes.
    let router = AppRouter::routes_with_state(state);

    axum::serve(listener, router).await.unwrap();
}
