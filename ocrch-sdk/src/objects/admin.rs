//! Admin API request and response types.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::blockchains::{Blockchain, Stablecoin};
use super::webhook::{OrderStatus, TransferStatus};

// ---------------------------------------------------------------------------
// Responses
// ---------------------------------------------------------------------------

/// Full order detail for admin API (includes webhook info).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminOrderResponse {
    /// Internal order ID.
    pub order_id: Uuid,
    /// Merchant-assigned order identifier.
    pub merchant_order_id: String,
    /// Payment amount in the selected stablecoin.
    pub amount: rust_decimal::Decimal,
    /// Current order status.
    pub status: OrderStatus,
    /// Unix timestamp of when the order was created.
    pub created_at: i64,
    /// Merchant webhook URL for order status change events.
    pub webhook_url: String,
    /// Number of times the webhook has been attempted.
    pub webhook_retry_count: i32,
    /// Unix timestamp of the first successful webhook delivery, if any.
    pub webhook_success_at: Option<i64>,
    /// Unix timestamp of the most recent webhook attempt, if any.
    pub webhook_last_tried_at: Option<i64>,
}

/// Unified pending deposit response covering both ERC-20 and TRC-20.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminPendingDepositResponse {
    /// Internal deposit ID.
    pub id: i64,
    /// Order this deposit belongs to.
    pub order_id: Uuid,
    /// Blockchain the deposit is expected on.
    pub blockchain: Blockchain,
    /// Stablecoin expected for payment.
    pub token: Stablecoin,
    /// Sender address, if known.
    pub user_address: Option<String>,
    /// Receiving wallet address.
    pub wallet_address: String,
    /// Expected payment amount.
    pub value: rust_decimal::Decimal,
    /// Unix timestamp when scanning started.
    pub started_at: i64,
    /// Unix timestamp of the most recent blockchain scan.
    pub last_scanned_at: i64,
}

/// Unified transfer response covering both ERC-20 and TRC-20.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminTransferResponse {
    /// Internal transfer ID.
    pub id: i64,
    /// Blockchain the transfer was observed on.
    pub blockchain: Blockchain,
    /// Stablecoin transferred.
    pub token: Stablecoin,
    /// Sender address.
    pub from_address: String,
    /// Recipient address.
    pub to_address: String,
    /// Transaction hash.
    pub txn_hash: String,
    /// Amount transferred.
    pub value: rust_decimal::Decimal,
    /// Block number containing the transaction.
    pub block_number: i64,
    /// Unix timestamp of the block.
    pub block_timestamp: i64,
    /// Whether the transaction has reached the required confirmation depth.
    pub blockchain_confirmed: bool,
    /// Unix timestamp when this record was first created.
    pub created_at: i64,
    /// Transfer matching / confirmation status.
    pub status: TransferStatus,
    /// Linked fulfillment ID, if this transfer matched a deposit.
    pub fulfillment_id: Option<i64>,
}

/// Wallet info from config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminWalletResponse {
    /// Blockchain this wallet belongs to.
    pub blockchain: Blockchain,
    /// Wallet address.
    pub address: String,
    /// Stablecoins enabled for this wallet.
    pub enabled_coins: Vec<Stablecoin>,
}

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 200;
const MAX_OFFSET: i64 = 100_000;

/// Query parameters for listing orders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListOrdersQuery {
    /// Maximum number of results to return (default 20, max 200).
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Number of results to skip for pagination.
    #[serde(default)]
    pub offset: i64,
    /// Filter by order status.
    pub status: Option<OrderStatus>,
    /// Filter by merchant-assigned order ID.
    pub merchant_order_id: Option<String>,
}

/// Query parameters for listing pending deposits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListDepositsQuery {
    /// Maximum number of results to return (default 20, max 200).
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Number of results to skip for pagination.
    #[serde(default)]
    pub offset: i64,
    /// Filter by order ID.
    pub order_id: Option<Uuid>,
    /// Filter by blockchain.
    pub blockchain: Option<Blockchain>,
    /// Filter by stablecoin.
    pub token: Option<Stablecoin>,
}

/// Query parameters for listing transfers by wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTransfersQuery {
    /// Maximum number of results to return (default 20, max 200).
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Number of results to skip for pagination.
    #[serde(default)]
    pub offset: i64,
    /// Filter by transfer status.
    pub status: Option<TransferStatus>,
    /// Filter by blockchain.
    pub blockchain: Option<Blockchain>,
    /// Filter by stablecoin.
    pub token: Option<Stablecoin>,
}

fn default_limit() -> i64 {
    DEFAULT_LIMIT
}

/// Clamp limit and offset to safe maximums.
pub fn clamp_pagination(limit: i64, offset: i64) -> (i64, i64) {
    (limit.clamp(1, MAX_LIMIT), offset.clamp(0, MAX_OFFSET))
}
