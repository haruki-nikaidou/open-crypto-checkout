//! Order creation and status types used by the Service API.

use crate::objects::blockchains;
use crate::objects::webhook::OrderStatus;
use crate::signature::Signature;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request payload for creating a new order.
///
/// Sent by the application backend to the Service API.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaymentCreatingEssential {
    /// Payment amount in the selected stablecoin.
    pub amount: rust_decimal::Decimal,
    /// Optional wallet address to restrict which address the user must pay from.
    pub expecting_wallet_address: Option<String>,
    /// Merchant-assigned order identifier (opaque string).
    pub order_id: String,
    /// Pre-selected blockchain, or `None` to let the user choose.
    pub blockchain: Option<blockchains::Blockchain>,
    /// Pre-selected stablecoin, or `None` to let the user choose.
    pub stablecoin: Option<blockchains::Stablecoin>,
    /// URL that the Ocrch server will POST webhook events to.
    pub webhook_url: String,
}

impl Signature for PaymentCreatingEssential {}

/// Request payload for getting the status of an existing order.
///
/// Sent by the application backend to the Service API.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GetOrderRequest {
    /// Internal Ocrch order ID returned when the order was created.
    pub order_id: Uuid,
}

impl Signature for GetOrderRequest {}

/// Response returned by both the "create order" and "get order status" endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    /// Internal order ID (UUID).
    pub order_id: Uuid,
    /// Merchant-provided order identifier.
    pub merchant_order_id: String,
    /// Payment amount.
    pub amount: rust_decimal::Decimal,
    /// Current order status.
    pub status: OrderStatus,
    /// Unix timestamp of when the order was created.
    pub created_at: i64,
}
