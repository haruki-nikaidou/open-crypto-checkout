//! Signal handling for graceful shutdown and config reload.

use crate::config::ConfigLoader;
use crate::state::AppState;
use std::sync::Arc;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::Notify;

/// Creates a future that completes when a shutdown signal is received.
///
/// Listens for SIGTERM and SIGINT (Ctrl+C).
pub async fn shutdown_signal() {
    let mut sigterm =
        signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
    let mut sigint =
        signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");

    tokio::select! {
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM, initiating graceful shutdown");
        }
        _ = sigint.recv() => {
            tracing::info!("Received SIGINT, initiating graceful shutdown");
        }
    }
}

/// Spawns a task that listens for SIGHUP and reloads the configuration.
///
/// Returns a Notify that can be used to signal when shutdown is complete.
pub fn spawn_config_reload_handler(
    state: AppState,
    config_loader: Arc<ConfigLoader>,
) -> Arc<Notify> {
    let shutdown_notify = Arc::new(Notify::new());
    let shutdown_notify_clone = shutdown_notify.clone();

    tokio::spawn(async move {
        let mut sighup = signal(SignalKind::hangup()).expect("failed to install SIGHUP handler");

        loop {
            tokio::select! {
                _ = sighup.recv() => {
                    tracing::info!("Received SIGHUP, reloading configuration");
                    match config_loader.load() {
                        Ok(new_config) => {
                            state.update_config(new_config).await;
                            tracing::info!("Configuration reloaded successfully");
                        }
                        Err(e) => {
                            tracing::error!("Failed to reload configuration: {}", e);
                        }
                    }
                }
                _ = shutdown_notify_clone.notified() => {
                    tracing::debug!("Config reload handler shutting down");
                    break;
                }
            }
        }
    });

    shutdown_notify
}
