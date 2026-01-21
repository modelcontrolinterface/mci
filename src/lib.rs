use aws_sdk_s3::Client;
use axum::Router;
use axum_server::{tls_rustls::RustlsConfig, Handle};
use futures::Future;
use std::{net::SocketAddr, path::PathBuf};
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

pub mod api;
pub mod config;
pub mod db;
pub mod errors;
pub mod models;
pub mod s3;
pub mod schema;
pub mod services;
pub mod utils;

#[derive(Clone)]
pub struct AppState {
    pub db_pool: db::PgPool,
    pub s3_client: Client,
}

pub fn app(app_state: AppState) -> Router {
    Router::new()
        .merge(api::routes::routes())
        .layer(TraceLayer::new_for_http())
        .with_state(app_state)
}

pub async fn serve(
    config: &config::Config,
    handle: Handle<std::net::SocketAddr>,
) -> Result<
    (impl Future<Output = Result<(), std::io::Error>>, SocketAddr),
    Box<dyn std::error::Error>,
> {
    let db_pool = db::create_pool(&config.database_url);
    let s3_client =
        s3::create_s3_client(&config.s3_url, &config.s3_access_key, &config.s3_secret_key).await;

    let mut conn = db_pool
        .get()
        .map_err(|e| format!("Failed to get database connection: {}", e))?;

    tokio::task::spawn_blocking(move || db::run_migrations(&mut conn))
        .await
        .map_err(|e| format!("Migration task panicked: {}", e))?
        .map_err(|e| format!("Failed to run migrations: {}", e))?;

    info!("Database migrations completed successfully");

    let app = app(AppState { db_pool, s3_client });

    let addr: SocketAddr = config
        .address
        .parse()
        .map_err(|e| format!("Invalid address '{}': {}", config.address, e))?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("Failed to bind to {}: {}", addr, e))?;
    let actual_addr = listener.local_addr()?;
    let std_listener = listener.into_std()?;
    let cert_path = config.cert_path.clone();
    let key_path = config.key_path.clone();

    let server_future = async move {
        if let (Some(cert_path), Some(key_path)) = (cert_path, key_path) {
            info!("Starting TLS server on {}", actual_addr);

            let tls_config =
                RustlsConfig::from_pem_file(PathBuf::from(cert_path), PathBuf::from(key_path))
                    .await
                    .map_err(std::io::Error::other)?;

            axum_server::from_tcp_rustls(std_listener, tls_config)
                .map_err(std::io::Error::other)?
                .handle(handle)
                .serve(app.into_make_service())
                .await
        } else {
            warn!("TLS certificates not provided. Starting insecure HTTP server.");
            info!("Starting HTTP server on {}", actual_addr);

            axum_server::from_tcp(std_listener)
                .map_err(std::io::Error::other)?
                .handle(handle)
                .serve(app.into_make_service())
                .await
        }
    };

    Ok((server_future, actual_addr))
}
