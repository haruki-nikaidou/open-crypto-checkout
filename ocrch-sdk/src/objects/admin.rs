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
    pub order_id: Uuid,
    pub merchant_order_id: String,
    pub amount: rust_decimal::Decimal,
    pub status: OrderStatus,
    pub created_at: i64,
    pub webhook_url: String,
    pub webhook_retry_count: i32,
    pub webhook_success_at: Option<i64>,
    pub webhook_last_tried_at: Option<i64>,
}

/// Unified pending deposit response covering both ERC-20 and TRC-20.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminPendingDepositResponse {
    pub id: i64,
    pub order_id: Uuid,
    pub blockchain: Blockchain,
    pub token: Stablecoin,
    pub user_address: Option<String>,
    pub wallet_address: String,
    pub value: rust_decimal::Decimal,
    pub started_at: i64,
    pub last_scanned_at: i64,
}

/// Unified transfer response covering both ERC-20 and TRC-20.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminTransferResponse {
    pub id: i64,
    pub blockchain: Blockchain,
    pub token: Stablecoin,
    pub from_address: String,
    pub to_address: String,
    pub txn_hash: String,
    pub value: rust_decimal::Decimal,
    pub block_number: i64,
    pub block_timestamp: i64,
    pub blockchain_confirmed: bool,
    pub created_at: i64,
    pub status: TransferStatus,
    pub fulfillment_id: Option<i64>,
}

/// Wallet info from config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminWalletResponse {
    pub blockchain: Blockchain,
    pub address: String,
    pub enabled_coins: Vec<Stablecoin>,
}

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 200;
const MAX_OFFSET: i64 = 100_000;

/// Query parameters for listing orders.
#[derive(Debug, Clone, Deserialize)]
pub struct ListOrdersQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub status: Option<OrderStatus>,
    pub merchant_order_id: Option<String>,
}

/// Query parameters for listing pending deposits.
#[derive(Debug, Clone, Deserialize)]
pub struct ListDepositsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub order_id: Option<Uuid>,
    pub blockchain: Option<Blockchain>,
    pub token: Option<Stablecoin>,
}

/// Query parameters for listing transfers by wallet.
#[derive(Debug, Clone, Deserialize)]
pub struct ListTransfersQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub status: Option<TransferStatus>,
    pub blockchain: Option<Blockchain>,
    pub token: Option<Stablecoin>,
}

fn default_limit() -> i64 {
    DEFAULT_LIMIT
}

/// Clamp limit and offset to safe maximums.
pub fn clamp_pagination(limit: i64, offset: i64) -> (i64, i64) {
    (limit.clamp(1, MAX_LIMIT), offset.clamp(0, MAX_OFFSET))
}
