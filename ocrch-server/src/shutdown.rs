//! Signal handling for graceful shutdown and config reload.

use crate::config::ConfigLoader;
use crate::state::AppState;
use ocrch_core::config::ConfigStore;
use ocrch_core::events::pooling_tick_channel;
use ocrch_core::processors::{PoolingKey, PoolingManagerConfig};
use ocrch_sdk::objects::blockchains::Blockchain;
use std::sync::Arc;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::Notify;

/// Creates a future that completes when a shutdown signal is received.
///
/// Listens for SIGTERM and SIGINT (Ctrl+C).
pub async fn shutdown_signal() {
    let mut sigterm = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");

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
    pooling_config_store: ConfigStore<PoolingManagerConfig>,
) -> Arc<Notify> {
    let shutdown_notify = Arc::new(Notify::new());
    let shutdown_notify_clone = shutdown_notify.clone();

    tokio::spawn(async move {
        let mut sighup = signal(SignalKind::hangup()).expect("failed to install SIGHUP handler");

        loop {
            tokio::select! {
                _ = sighup.recv() => {
                    tracing::info!("Received SIGHUP, reloading configuration");
                    match config_loader.reload() {
                        Ok(loaded_config) => {
                            // Update all config sections
                            *state.config.server.write().await = loaded_config.server;
                            *state.config.admin.write().await = loaded_config.admin;
                            *state.config.merchant.write().await = loaded_config.merchant;
                            *state.config.wallets.write().await = loaded_config.wallets.clone();
                            *state.config.api_keys.write().await = loaded_config.api_keys;

                            // Rebuild PoolingManagerConfig from new wallets so
                            // PoolingManager can diff and reconcile tick loops.
                            let new_tick_senders = loaded_config
                                .wallets
                                .iter()
                                .flat_map(|w| {
                                    w.enabled_coins.iter().map(move |coin| {
                                        let target = blockchain_to_target(w.blockchain);
                                        let token = (*coin).into();
                                        let key = PoolingKey::new(target, token);
                                        let (tx, _rx) = pooling_tick_channel();
                                        (key, tx)
                                    })
                                })
                                .collect();
                            pooling_config_store
                                .update(PoolingManagerConfig {
                                    tick_senders: new_tick_senders,
                                })
                                .await;

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

/// Map an SDK `Blockchain` variant to a `BlockchainTarget` for the event system.
fn blockchain_to_target(blockchain: Blockchain) -> ocrch_core::events::BlockchainTarget {
    match blockchain {
        Blockchain::Tron => ocrch_core::events::BlockchainTarget::Trc20,
        other => ocrch_core::events::BlockchainTarget::Erc20(blockchain_to_etherscan_chain(other)),
    }
}

/// Map an SDK `Blockchain` variant to an `EtherScanChain`.
fn blockchain_to_etherscan_chain(
    blockchain: Blockchain,
) -> ocrch_core::entities::erc20_pending_deposit::EtherScanChain {
    use ocrch_core::entities::erc20_pending_deposit::EtherScanChain;
    match blockchain {
        Blockchain::Ethereum => EtherScanChain::Ethereum,
        Blockchain::Polygon => EtherScanChain::Polygon,
        Blockchain::Base => EtherScanChain::Base,
        Blockchain::ArbitrumOne => EtherScanChain::ArbitrumOne,
        Blockchain::Linea => EtherScanChain::Linea,
        Blockchain::Optimism => EtherScanChain::Optimism,
        Blockchain::AvalancheC => EtherScanChain::AvalancheC,
        Blockchain::Tron => panic!("Tron is not an EtherScan chain"),
    }
}
