pub mod connection;

use crate::cache::storage::CacheStorage;
use crate::server::connection::Connection;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing::{info, warn, error};

/// FerroCache server configuration
pub struct ServerConfig {
    pub bind_addr: String,
    pub memory_limit: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:6379".to_string(),
            memory_limit: 100 * 1024 * 1024, // 100 MB
        }
    }
}

/// Main server that accepts connections and spawns handlers
pub struct Server {
    config: ServerConfig,
    cache: Arc<CacheStorage>,
    shutdown_tx: broadcast::Sender<()>,
}

impl Server {
    /// Create a new server with the given configuration
    pub fn new(config: ServerConfig) -> Self {
        let cache = Arc::new(CacheStorage::new(config.memory_limit));
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            config,
            cache,
            shutdown_tx,
        }
    }

    /// Get a reference to the cache (useful for testing)
    pub fn cache(&self) -> &Arc<CacheStorage> {
        &self.cache
    }

    /// Get a shutdown signal receiver
    pub fn subscribe_shutdown(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Run the server (accepts connections until shutdown signal)
    ///
    /// This is the main server loop that:
    /// 1. Binds to the configured address
    /// 2. Accepts incoming connections
    /// 3. Spawns a task for each connection
    /// 4. Waits for shutdown signal
    pub async fn run(&self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(&self.config.bind_addr).await?;
        info!("FerroCache listening on {}", self.config.bind_addr);

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                // Accept new connections
                result = listener.accept() => {
                    let (stream, addr) = result?;
                    info!("New connection from {}", addr);

                    let cache = self.cache.clone();
                    let mut conn_shutdown_rx = self.shutdown_tx.subscribe();

                    tokio::spawn(async move {
                        tokio::select! {
                            result = Connection::handle(stream, cache) => {
                                match result {
                                    Ok(()) => info!("Connection from {} closed", addr),
                                    Err(e) => error!("Connection error from {}: {}", addr, e),
                                }
                            }
                            _ = conn_shutdown_rx.recv() => {
                                warn!("Shutdown signal received, closing connection from {}", addr);
                            }
                        }
                    });
                }

                // Shutdown signal (triggered by calling shutdown())
                _ = shutdown_rx.recv() => {
                    info!("Server shutting down...");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Trigger graceful shutdown
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}
