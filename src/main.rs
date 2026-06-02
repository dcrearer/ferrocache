use ferrocache::server::{Server, ServerConfig};
use ferrocache::expiration::reaper::ExpirationReaper;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tracing::{info, warn, error};
use tracing_subscriber::{EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"))
        )
        .init();

    // Parse config (for now, use defaults)
    let config = ServerConfig::default();

    info!("Starting FerroCache...");
    info!("Memory limit: {} MB", config.memory_limit / (1024 * 1024));
    info!("Binding to: {}", config.bind_addr);

    // Create server wrapped in Arc for shared ownership
    let server = Arc::new(Server::new(config));

    // Start expiration reaper (background task)
    let reaper = ExpirationReaper::new(
        server.cache().clone(),
        Duration::from_secs(60), // Scan every 60 seconds
    );
    let reaper_handle = tokio::spawn(reaper.run());

    // Run server with graceful shutdown
    let server_clone = server.clone();
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server_clone.run().await {
            error!("Server error: {}", e);
        }
    });

    // Wait for Ctrl+C
    signal::ctrl_c().await?;
    info!("\nReceived Ctrl+C, shutting down gracefully...");

    // Trigger shutdown
    server.shutdown();

    // Wait for server to finish (with timeout)
    let shutdown_timeout = Duration::from_secs(2);
    tokio::select! {
        _ = server_handle => {
            info!("Server shut down cleanly");
        }
        _ = tokio::time::sleep(shutdown_timeout) => {
            warn!("Shutdown timeout reached");
        }
    }

    // Stop reaper
    reaper_handle.abort();

    info!("Goodbye!");
    Ok(())
}
