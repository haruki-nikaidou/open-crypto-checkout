//! Webhook payload types for order and transfer events.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::signature::Signature;

/// Webhook payload for order status change events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderStatusChangedPayload {
    /// Event type identifier (e.g. `"order_status_changed"`).
    pub event_type: String,
    /// Internal order ID.
    pub order_id: Uuid,
    /// Merchant-assigned order identifier.
    pub merchant_order_id: String,
    /// New order status.
    pub status: OrderStatus,
    /// Payment amount as a string.
    pub amount: String,
    /// Unix timestamp of when the event was emitted.
    pub timestamp: i64,
}

impl Signature for OrderStatusChangedPayload {}

/// Webhook payload for unknown transfer events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnknownTransferPayload {
    /// Event type identifier (e.g. `"unknown_transfer"`).
    pub event_type: String,
    /// Internal transfer ID.
    pub transfer_id: i64,
    /// Blockchain the transfer was observed on (as a string).
    pub blockchain: String,
    /// Unix timestamp of when the event was emitted.
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
    /// Order is awaiting payment.
    Pending,
    /// Order has been successfully paid.
    Paid,
    /// Order expired before payment was received.
    Expired,
    /// Order was cancelled by the user or merchant.
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
    /// Transfer detected on-chain but not yet confirmed to the required depth.
    WaitingForConfirmation,
    /// Transfer could not reach the required confirmation depth.
    FailedToConfirm,
    /// Transfer confirmed but not yet matched to a pending deposit.
    WaitingForMatch,
    /// Transfer confirmed but no matching deposit was found.
    NoMatchedDeposit,
    /// Transfer confirmed and matched to a pending deposit.
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
