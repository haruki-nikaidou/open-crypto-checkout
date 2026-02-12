//! PoolingManager processor.
//!
//! The PoolingManager is responsible for:
//! - Receiving `PendingDepositChanged` events via the `Processor` trait
//! - Tracking the latest pending deposit timestamp per (blockchain, token) pair
//! - Emitting `PoolingTick` events on a calculated schedule
//! - Reacting to config changes by diffing active tasks (spawning/aborting
//!   only the tasks that actually changed)
//!
//! The pooling frequency is calculated based on how recently there was activity,
//! using the `pooling_freq` function from the utils module.

use crate::config::{ConfigStore, ConfigWatcher};
use crate::entities::StablecoinName;
use crate::events::{
    BlockchainTarget, PendingDepositChanged, PendingDepositChangedReceiver, PoolingTick,
    PoolingTickSender,
};
use crate::utils::pooling_interval::pooling_freq;
use kanau::processor::Processor;
use std::convert::Infallible;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

/// Key for identifying a blockchain-token pair.
///
/// Since the total number of pairs is small (chains x tokens), this is stored
/// in a `Vec` and searched linearly rather than hashed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolingKey {
    pub blockchain: BlockchainTarget,
    pub token: StablecoinName,
}

impl PoolingKey {
    pub fn new(blockchain: BlockchainTarget, token: StablecoinName) -> Self {
        Self { blockchain, token }
    }
}

/// Configuration for the PoolingManager.
///
/// Contains the list of active (blockchain, token) pairs and the
/// corresponding tick senders. Held inside a [`ConfigStore`] so that
/// it can be swapped at runtime.
pub struct PoolingManagerConfig {
    /// Active blockchain-token pairs and their tick senders.
    ///
    /// Stored as a `Vec` because the number of pairs is very small,
    /// making linear scans faster than hash lookups.
    pub tick_senders: Vec<(PoolingKey, PoolingTickSender)>,
}

// ---------------------------------------------------------------------------
// PoolingManager
// ---------------------------------------------------------------------------

/// PoolingManager handles scheduling of blockchain sync operations.
///
/// It maintains a pooling schedule for each enabled (blockchain, token) pair,
/// adjusting the frequency based on recent pending deposit activity.
///
/// All signal receivers (`shutdown_rx`, `event_rx`, `config_watcher`) are
/// injected when calling [`run()`](PoolingManager::run) rather than owned
/// by the struct, following the same pattern as the other processors.
pub struct PoolingManager {
    /// Broadcast channel for notifying tick loops of timestamp updates.
    update_tx: broadcast::Sender<(PoolingKey, time::PrimitiveDateTime)>,
}

impl PoolingManager {
    /// Create a new PoolingManager.
    pub fn new() -> Self {
        let (update_tx, _) = broadcast::channel(64);
        Self { update_tx }
    }

    /// Run the PoolingManager until shutdown is signaled.
    ///
    /// This method:
    /// 1. Reads the initial config and spawns tick-loop tasks
    /// 2. Listens for `PendingDepositChanged` events and broadcasts
    ///    timestamp updates to the tick loops
    /// 3. Reacts to config changes by diffing and spawning/aborting
    ///    only the tasks that actually changed
    /// 4. Shuts down gracefully when the shutdown signal fires
    pub async fn run(
        self,
        mut shutdown_rx: watch::Receiver<bool>,
        mut event_rx: PendingDepositChangedReceiver,
        config_store: ConfigStore<PoolingManagerConfig>,
        mut config_watcher: ConfigWatcher,
    ) {
        // -- Bootstrap from initial config ----------------------------------
        let mut active_tasks: Vec<(PoolingKey, JoinHandle<()>)> = Vec::new();
        {
            let config = config_store.read().await;
            for (key, sender) in &config.tick_senders {
                let handle = self.spawn_tick_loop(*key, sender.clone());
                active_tasks.push((*key, handle));
            }
            info!(
                "PoolingManager started with {} blockchain-token pairs",
                active_tasks.len()
            );
        }

        // -- Main event loop ------------------------------------------------
        loop {
            tokio::select! {
                biased;

                // Shutdown has highest priority.
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("PoolingManager received shutdown signal");
                        break;
                    }
                }

                // Config changed — diff and reconcile.
                Ok(()) = config_watcher.changed() => {
                    let config = config_store.read().await;
                    self.apply_diff(&mut active_tasks, &config);
                    info!(
                        "PoolingManager reconciled config, {} active pairs",
                        active_tasks.len()
                    );
                }

                // Incoming event — delegate to Processor impl.
                Some(event) = event_rx.recv() => {
                    let _ = self.process(event).await;
                }

                // All senders dropped.
                else => {
                    info!("PendingDepositChanged channel closed");
                    break;
                }
            }
        }

        // -- Cleanup --------------------------------------------------------
        for (_, handle) in active_tasks {
            handle.abort();
        }

        info!("PoolingManager shutdown complete");
    }

    // -- Private helpers ----------------------------------------------------

    /// Diff `active` tasks against `new_config` and reconcile:
    /// - Abort tasks whose keys are absent from the new config.
    /// - Spawn tasks for keys present in the new config but not yet active.
    fn apply_diff(
        &self,
        active: &mut Vec<(PoolingKey, JoinHandle<()>)>,
        new_config: &PoolingManagerConfig,
    ) {
        // 1. Remove tasks whose keys are absent from new config.
        active.retain(|(key, handle)| {
            let keep = new_config.tick_senders.iter().any(|(k, _)| k == key);
            if !keep {
                info!(blockchain = %key.blockchain, token = ?key.token, "Aborting removed tick loop");
                handle.abort();
            }
            keep
        });

        // 2. Spawn tasks for newly added keys.
        for (key, sender) in &new_config.tick_senders {
            if !active.iter().any(|(k, _)| k == key) {
                info!(blockchain = %key.blockchain, token = ?key.token, "Spawning new tick loop");
                let handle = self.spawn_tick_loop(*key, sender.clone());
                active.push((*key, handle));
            }
        }
    }

    /// Spawn a tick-loop task for a single blockchain-token pair.
    ///
    /// The spawned task runs an adaptive-interval loop using [`pooling_freq`]:
    /// it sleeps for the calculated interval, emits a `PoolingTick`, then
    /// recalculates. If a timestamp update arrives via the broadcast channel,
    /// the interval is recalculated immediately.
    fn spawn_tick_loop(&self, key: PoolingKey, tick_sender: PoolingTickSender) -> JoinHandle<()> {
        let mut update_rx = self.update_tx.subscribe();
        let blockchain = key.blockchain;
        let token = key.token;

        tokio::spawn(async move {
            let mut last_pending_at = time::PrimitiveDateTime::MIN;

            loop {
                let now = time::OffsetDateTime::now_utc();
                let now = time::PrimitiveDateTime::new(now.date(), now.time());
                let next_interval = pooling_freq(last_pending_at, now);
                let sleep_duration =
                    std::time::Duration::from_secs(next_interval.whole_seconds() as u64);

                debug!(
                    %blockchain,
                    ?token,
                    %last_pending_at,
                    %now,
                    %next_interval,
                    "Calculating next pooling interval"
                );

                tokio::select! {
                    biased;

                    // Timestamp update from the Processor impl.
                    Ok((updated_key, new_ts)) = update_rx.recv() => {
                        if updated_key == key {
                            last_pending_at = new_ts;
                            debug!(
                                %blockchain,
                                ?token,
                                "Updated last_pending_at, recalculating interval"
                            );
                            // Skip sleep and immediately recalculate.
                            continue;
                        }
                    }

                    // Interval elapsed — emit a tick.
                    _ = tokio::time::sleep(sleep_duration) => {
                        let tick = PoolingTick { blockchain, token };
                        if let Err(e) = tick_sender.send(tick).await {
                            warn!(
                                %blockchain,
                                ?token,
                                error = %e,
                                "Failed to send PoolingTick, receiver dropped"
                            );
                            return;
                        }
                        debug!(%blockchain, ?token, "Emitted PoolingTick");
                    }
                }
            }
        })
    }
}

impl Default for PoolingManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Processor trait implementation
// ---------------------------------------------------------------------------

impl Processor<PendingDepositChanged> for PoolingManager {
    type Output = ();
    type Error = Infallible;

    async fn process(&self, event: PendingDepositChanged) -> Result<(), Infallible> {
        let key = PoolingKey::new(event.blockchain_target(), event.token());
        let now = time::OffsetDateTime::now_utc();
        let now = time::PrimitiveDateTime::new(now.date(), now.time());

        debug!(
            blockchain = %key.blockchain,
            token = ?key.token,
            "Received PendingDepositChanged, broadcasting timestamp update"
        );

        // Broadcast to tick loops; if nobody is listening yet that is fine.
        let _ = self.update_tx.send((key, now));
        Ok(())
    }
}
