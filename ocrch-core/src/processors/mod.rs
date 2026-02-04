//! Event processors for the event-driven architecture.
//!
//! This module contains all the processors that handle events in the system:
//!
//! - `PoolingManager`: Receives `PendingDepositChanged`, emits `PoolingTick`
//! - `BlockchainSync`: Receives `PoolingTick`, emits `MatchTick`
//! - `OrderBookWatcher`: Receives `MatchTick`, emits `WebhookEvent`
//! - `WebhookSender`: Receives `WebhookEvent`, delivers webhooks

pub mod blockchain_sync;
pub mod order_watcher;
pub mod pooling_manager;
pub mod webhook_sender;

pub use blockchain_sync::{BlockchainSync, Erc20BlockchainSync, SyncError, Trc20BlockchainSync};
pub use order_watcher::OrderBookWatcher;
pub use pooling_manager::PoolingManager;
pub use webhook_sender::WebhookSender;
