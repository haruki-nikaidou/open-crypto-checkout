use crate::objects::Signature;
use crate::objects::blockchains;
use crate::objects::webhook::OrderStatus;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request payload for creating a new order.
///
/// Sent by the application backend to the Service API.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaymentCreatingEssential {
    pub amount: rust_decimal::Decimal,
    pub expecting_wallet_address: Option<String>,
    pub order_id: String,
    pub blockchain: Option<blockchains::Blockchain>,
    pub stablecoin: Option<blockchains::Stablecoin>,
    pub webhook_url: String,
}

impl Signature for PaymentCreatingEssential {}

/// Request payload for getting the status of an existing order.
///
/// Sent by the application backend to the Service API.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GetOrderRequest {
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
