use ferrocache::config::Config;
use ferrocache::server::{Server, ServerConfig};
use ferrocache::expiration::reaper::ExpirationReaper;
use ferrocache::telemetry;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tracing::{info, warn, error};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration: defaults -> TOML file -> env overrides. This happens
    // before telemetry init so logging is configured from the file too.
    let config = Config::load()?;

    // Initialize logging + (optional) OTLP trace export. The guard must live
    // for the whole program: dropping it flushes buffered spans on exit.
    let _telemetry = telemetry::init(&config.observability)?;

    let server_config = ServerConfig {
        bind_addr: config.server.bind_addr.clone(),
        memory_limit: config.server.memory_limit_bytes(),
    };

    info!("Starting FerroCache...");
    info!("Memory limit: {} MB", config.server.memory_limit_mb);
    info!("Binding to: {}", server_config.bind_addr);
    if let Some(endpoint) = &config.observability.otlp_endpoint {
        info!(
            "OTLP export enabled: {} (trace sample ratio {:.3}, metric interval {}s; \
             collector-down is non-fatal)",
            endpoint,
            config.observability.sample_ratio_clamped(),
            config.observability.metric_export_interval_secs,
        );
    } else {
        info!("OTLP export disabled (stdout logging only)");
    }

    // Create server wrapped in Arc for shared ownership
    let server = Arc::new(Server::new(server_config));

    // Register observable cache metrics (hit/miss/eviction/memory/keys). These
    // read the cache's lock-free atomics off the hot path on the OTel SDK's
    // collection interval. No-op if OTLP export is disabled.
    telemetry::register_cache_metrics(server.cache());

    // Start expiration reaper (background task)
    let reaper = ExpirationReaper::new(
        server.cache().clone(),
        Duration::from_secs(config.server.reaper_interval_secs),
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
