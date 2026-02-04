//! Event type definitions for the event-driven architecture.
//!
//! All events in the system are idempotent and ephemeral. They carry
//! identifiers rather than full data, requiring processors to fetch
//! current state from the database.

use crate::entities::erc20_pending_deposit::EtherScanChain;
use crate::entities::order_records::OrderStatus;
use crate::entities::StablecoinName;
use uuid::Uuid;

/// Unified blockchain target for event routing.
///
/// This enum represents the target blockchain for pooling and sync operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockchainTarget {
    /// ERC-20 compatible chains (Ethereum, Polygon, Base, etc.)
    Erc20(EtherScanChain),
    /// TRC-20 (Tron network)
    Trc20,
}

impl std::fmt::Display for BlockchainTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockchainTarget::Erc20(chain) => write!(f, "erc20:{:?}", chain),
            BlockchainTarget::Trc20 => write!(f, "trc20"),
        }
    }
}

/// Event emitted when a pending deposit changes status.
///
/// This is the entry point for the event flow. It triggers the PoolingManager
/// to recalculate pooling frequency for the affected blockchain-token pair.
#[derive(Debug, Clone)]
pub enum PendingDepositChanged {
    /// ERC-20 pending deposit changed
    Erc20 {
        deposit_id: i64,
        chain: EtherScanChain,
        token: StablecoinName,
    },
    /// TRC-20 pending deposit changed
    Trc20 {
        deposit_id: i64,
        token: StablecoinName,
    },
}

impl PendingDepositChanged {
    /// Get the blockchain target for this event.
    pub fn blockchain_target(&self) -> BlockchainTarget {
        match self {
            PendingDepositChanged::Erc20 { chain, .. } => BlockchainTarget::Erc20(*chain),
            PendingDepositChanged::Trc20 { .. } => BlockchainTarget::Trc20,
        }
    }

    /// Get the token for this event.
    pub fn token(&self) -> StablecoinName {
        match self {
            PendingDepositChanged::Erc20 { token, .. } => *token,
            PendingDepositChanged::Trc20 { token, .. } => *token,
        }
    }
}

/// Event emitted by PoolingManager to trigger blockchain sync.
///
/// Each enabled token on each blockchain has its own pooling schedule,
/// and this tick triggers the corresponding BlockchainSync to fetch new data.
#[derive(Debug, Clone)]
pub struct PoolingTick {
    /// The blockchain to sync
    pub blockchain: BlockchainTarget,
    /// The token to sync
    pub token: StablecoinName,
}

/// Event emitted by BlockchainSync after syncing data.
///
/// This triggers the OrderBookWatcher to attempt matching pending deposits
/// with the newly synced transfers.
#[derive(Debug, Clone)]
pub struct MatchTick {
    /// The blockchain that was synced
    pub blockchain: BlockchainTarget,
    /// The token that was synced
    pub token: StablecoinName,
    /// Number of new transfers synced (0 if no new data)
    pub transfers_synced: u32,
}

/// Events that trigger webhook delivery.
///
/// These events are sent to the WebhookSender for delivery to merchant endpoints.
#[derive(Debug, Clone)]
pub enum WebhookEvent {
    /// Order status has changed (paid, expired, canceled)
    OrderStatusChanged {
        order_id: Uuid,
        new_status: OrderStatus,
    },
    /// A transfer was received that doesn't match any pending deposit
    UnknownTransferReceived {
        transfer_id: i64,
        blockchain: BlockchainTarget,
    },
}
