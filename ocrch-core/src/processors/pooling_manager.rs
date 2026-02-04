//! PoolingManager processor.
//!
//! The PoolingManager is responsible for:
//! - Receiving `PendingDepositChanged` events
//! - Tracking the latest pending deposit timestamp per (blockchain, token) pair
//! - Emitting `PoolingTick` events on a calculated schedule
//!
//! The pooling frequency is calculated based on how recently there was activity,
//! using the `pooling_freq` function from the utils module.

use crate::entities::StablecoinName;
use crate::entities::erc20_pending_deposit::EtherScanChain;
use crate::events::{
    BlockchainTarget, PendingDepositChangedReceiver, PoolingTick, PoolingTickSender,
};
use crate::utils::pooling_interval::pooling_freq;
use std::collections::HashMap;
use tokio::sync::watch;
use tracing::{debug, info, warn};

/// State for a single blockchain-token pair's pooling schedule.
#[derive(Debug)]
struct PoolingState {
    /// Timestamp of the last pending deposit activity
    last_pending_at: time::PrimitiveDateTime,
    /// The sender for this blockchain-token pair's PoolingTick events
    tick_sender: PoolingTickSender,
}

/// Key for identifying a blockchain-token pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PoolingKey {
    blockchain: BlockchainTarget,
    token: StablecoinName,
}

impl PoolingKey {
    fn new(blockchain: BlockchainTarget, token: StablecoinName) -> Self {
        Self { blockchain, token }
    }
}

/// Configuration for the PoolingManager.
pub struct PoolingManagerConfig {
    /// Map of (blockchain, token) pairs to their tick senders.
    /// Each enabled blockchain-token combination should have an entry.
    pub tick_senders: HashMap<(BlockchainTarget, StablecoinName), PoolingTickSender>,
}

/// PoolingManager handles scheduling of blockchain sync operations.
///
/// It maintains a pooling schedule for each enabled (blockchain, token) pair,
/// adjusting the frequency based on recent pending deposit activity.
pub struct PoolingManager {
    /// Receiver for PendingDepositChanged events
    event_rx: PendingDepositChangedReceiver,
    /// Shutdown signal receiver
    shutdown_rx: watch::Receiver<bool>,
    /// Pooling state for each blockchain-token pair
    pooling_states: HashMap<PoolingKey, PoolingState>,
}

impl PoolingManager {
    /// Create a new PoolingManager.
    ///
    /// # Arguments
    ///
    /// * `event_rx` - Receiver for PendingDepositChanged events
    /// * `shutdown_rx` - Receiver for shutdown signal
    /// * `config` - Configuration containing tick senders for each blockchain-token pair
    pub fn new(
        event_rx: PendingDepositChangedReceiver,
        shutdown_rx: watch::Receiver<bool>,
        config: PoolingManagerConfig,
    ) -> Self {
        // Initialize pooling states with default timestamps (epoch start means max interval)
        let pooling_states = config
            .tick_senders
            .into_iter()
            .map(|((blockchain, token), sender)| {
                let key = PoolingKey::new(blockchain, token);
                let state = PoolingState {
                    // Start with epoch to use maximum interval initially
                    last_pending_at: time::PrimitiveDateTime::MIN,
                    tick_sender: sender,
                };
                (key, state)
            })
            .collect();

        Self {
            event_rx,
            shutdown_rx,
            pooling_states,
        }
    }

    /// Run the PoolingManager.
    ///
    /// This method runs until shutdown is signaled. It:
    /// 1. Receives PendingDepositChanged events and updates timestamps
    /// 2. Emits PoolingTick events on calculated schedules
    pub async fn run(mut self) {
        info!(
            "PoolingManager started with {} blockchain-token pairs",
            self.pooling_states.len()
        );

        // Spawn a task for each pooling schedule
        let mut tick_handles = Vec::new();
        let (update_tx, _) =
            tokio::sync::broadcast::channel::<(PoolingKey, time::PrimitiveDateTime)>(64);

        async fn handle_task(
            last_pending_at: &mut time::PrimitiveDateTime,
            shutdown_rx: &mut watch::Receiver<bool>,
            update_rx: &mut tokio::sync::broadcast::Receiver<(PoolingKey, time::PrimitiveDateTime)>,
            blockchain: BlockchainTarget,
            token: StablecoinName,
            tick_sender: &PoolingTickSender,
        ) -> bool {
            let now = time::OffsetDateTime::now_utc();
            let now = time::PrimitiveDateTime::new(now.date(), now.time());
            let next_interval = pooling_freq(*last_pending_at, now);
            let sleep_duration =
                std::time::Duration::from_secs(next_interval.whole_seconds() as u64);
            debug!(
                last_pending_at = %last_pending_at,
                now = %now,
                next_interval = %next_interval,
                "Calculating next pooling interval"
            );
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!(blockchain = %blockchain, token = ?token, "PoolingManager tick loop shutting down");
                        return false;
                    }
                }
                Ok((updated_key, new_timestamp)) = update_rx.recv() => {
                    if updated_key.blockchain == blockchain && updated_key.token == token {
                        *last_pending_at = new_timestamp;
                        debug!(
                            blockchain = %blockchain,
                            token = ?token,
                            "Updated last_pending_at, recalculating interval"
                        );
                        // Don't sleep, immediately recalculate
                        return true;
                    }
                } // Ok(_,_) = update_rx.recv() =>

                // Wait for next tick
                _ = tokio::time::sleep(sleep_duration) => {
                    let tick = PoolingTick { blockchain, token };
                    if let Err(e) = tick_sender.send(tick).await {
                        warn!(
                            blockchain = %blockchain,
                            token = ?token,
                            error = %e,
                            "Failed to send PoolingTick, receiver dropped"
                        );
                        return false;
                    }
                    debug!(blockchain = %blockchain, token = ?token, "Emitted PoolingTick");
                } // _ = tokio::time::sleep(sleep_duration) =>
            } // tokio::select!
            true
        }

        for (key, state) in &self.pooling_states {
            let blockchain = key.blockchain;
            let token = key.token;
            let mut shutdown_rx = self.shutdown_rx.clone();
            let mut update_rx = update_tx.subscribe();
            let mut last_pending_at = state.last_pending_at;
            let tick_sender = state.tick_sender.clone();
            let handle = tokio::spawn(async move {
                loop {
                    if !handle_task(
                        &mut last_pending_at,
                        &mut shutdown_rx,
                        &mut update_rx,
                        blockchain,
                        token,
                        &tick_sender,
                    )
                    .await
                    {
                        break;
                    }
                }
            }); // let handle = tokio::spawn(async move { ... })
            tick_handles.push(handle);
        } // for (key, state) in &self.pooling_states

        // Main loop: receive PendingDepositChanged events and update timestamps
        loop {
            tokio::select! {
                biased;

                // Check for shutdown
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        info!("PoolingManager received shutdown signal");
                        break;
                    }
                }

                // Receive PendingDepositChanged events
                Some(event) = self.event_rx.recv() => {
                    let blockchain = event.blockchain_target();
                    let token = event.token();
                    let key = PoolingKey::new(blockchain, token);

                    let now = time::OffsetDateTime::now_utc();
                    let now_primitive = time::PrimitiveDateTime::new(now.date(), now.time());

                    if let Some(state) = self.pooling_states.get_mut(&key) {
                        state.last_pending_at = now_primitive;
                        debug!(
                            blockchain = %blockchain,
                            token = ?token,
                            "Received PendingDepositChanged, updated timestamp"
                        );

                        // Notify the tick loop about the update
                        let _ = update_tx.send((key, now_primitive));
                    } else {
                        warn!(
                            blockchain = %blockchain,
                            token = ?token,
                            "Received PendingDepositChanged for untracked blockchain-token pair"
                        );
                    }
                } // Some(event) = self.event_rx.recv() =>

                else => {
                    info!("PendingDepositChanged channel closed");
                    break;
                }
            } // tokio::select!
        } // loop

        // Wait for all tick loops to complete
        for handle in tick_handles {
            let _ = handle.await;
        }

        info!("PoolingManager shutdown complete");
    }
}

/// Builder for creating PoolingManager with configured blockchain-token pairs.
pub struct PoolingManagerBuilder {
    tick_senders: HashMap<(BlockchainTarget, StablecoinName), PoolingTickSender>,
}

impl PoolingManagerBuilder {
    /// Create a new PoolingManagerBuilder.
    pub fn new() -> Self {
        Self {
            tick_senders: HashMap::new(),
        }
    }

    /// Register an ERC-20 chain-token pair.
    pub fn with_erc20(
        mut self,
        chain: EtherScanChain,
        token: StablecoinName,
        sender: PoolingTickSender,
    ) -> Self {
        self.tick_senders
            .insert((BlockchainTarget::Erc20(chain), token), sender);
        self
    }

    /// Register a TRC-20 token.
    pub fn with_trc20(mut self, token: StablecoinName, sender: PoolingTickSender) -> Self {
        self.tick_senders
            .insert((BlockchainTarget::Trc20, token), sender);
        self
    }

    /// Build the PoolingManager.
    pub fn build(
        self,
        event_rx: PendingDepositChangedReceiver,
        shutdown_rx: watch::Receiver<bool>,
    ) -> PoolingManager {
        PoolingManager::new(
            event_rx,
            shutdown_rx,
            PoolingManagerConfig {
                tick_senders: self.tick_senders,
            },
        )
    }
}

impl Default for PoolingManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}
