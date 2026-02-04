//! Webhook payload types for order and transfer events.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Signature;

/// Webhook payload for order status change events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderStatusChangedPayload {
    pub event_type: String,
    pub order_id: Uuid,
    pub merchant_order_id: String,
    pub status: OrderStatus,
    pub amount: String,
    pub timestamp: i64,
}

impl Signature for OrderStatusChangedPayload {}

/// Webhook payload for unknown transfer events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnknownTransferPayload {
    pub event_type: String,
    pub transfer_id: i64,
    pub blockchain: String,
    pub timestamp: i64,
}

impl Signature for UnknownTransferPayload {}

/// Order status for API responses.
///
/// This is the API/DTO version without sqlx::Type.
/// For database operations, use the version in `ocrch-core::entities`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    Pending,
    Paid,
    Expired,
    Cancelled,
}

impl std::fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderStatus::Pending => write!(f, "pending"),
            OrderStatus::Paid => write!(f, "paid"),
            OrderStatus::Expired => write!(f, "expired"),
            OrderStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Transfer status for API responses.
///
/// This is the API/DTO version without sqlx::Type.
/// For database operations, use the version in `ocrch-core::entities`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    WaitingForConfirmation,
    FailedToConfirm,
    WaitingForMatch,
    NoMatchedDeposit,
    Matched,
}

impl std::fmt::Display for TransferStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferStatus::WaitingForConfirmation => write!(f, "waiting_for_confirmation"),
            TransferStatus::FailedToConfirm => write!(f, "failed_to_confirm"),
            TransferStatus::WaitingForMatch => write!(f, "waiting_for_match"),
            TransferStatus::NoMatchedDeposit => write!(f, "no_matched_deposit"),
            TransferStatus::Matched => write!(f, "matched"),
        }
    }
}
