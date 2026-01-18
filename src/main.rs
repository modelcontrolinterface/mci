use mci::{config::Config, serve};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let config = Config::from_env().expect("Failed to load configuration from environment");

    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter(EnvFilter::new(&config.log_level))
        .init();

    let handle = axum_server::Handle::new();
    let shutdown_handle = handle.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for Ctrl+C");

        info!("Shutdown signal received. Closing server gracefully...");

        shutdown_handle.graceful_shutdown(Some(std::time::Duration::from_secs(30)));
    });

    let (server_future, addr) = serve(&config, handle)
        .await
        .expect("Failed to start server");

    info!("Server running on {}", addr);

    server_future.await.expect("Server failed to run");
}
