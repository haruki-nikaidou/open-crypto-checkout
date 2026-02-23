//! WebSocket message types for the order status stream.
//!
//! The `GET /orders/{order_id}/ws` endpoint upgrades to a WebSocket
//! connection and pushes [`WsServerMessage`] JSON frames.
//!
//! # Protocol
//!
//! 1. The server sends a [`WsServerMessage::StatusUpdate`] with the
//!    current order status immediately after the upgrade.
//! 2. Subsequent [`WsServerMessage::StatusUpdate`] frames are sent
//!    whenever the order status changes.
//! 3. After a terminal status (`Paid`, `Expired`, `Cancelled`) the
//!    server sends a normal close frame.
//! 4. If the order is not found or an internal error occurs *during*
//!    the handshake phase, the server sends a close frame with an
//!    application-defined close code (see [`WsCloseCode`]).

use serde::{Deserialize, Serialize};

use super::create_payment::OrderResponse;

/// Server-to-client WebSocket message.
///
/// Serialized as an internally-tagged JSON object so the client can
/// dispatch on the `"type"` field:
///
/// ```json
/// {"type":"status_update","order":{ ... }}
/// {"type":"error","code":4004,"reason":"order not found"}
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsServerMessage {
    /// An order status snapshot (sent as the first frame and on every
    /// subsequent change).
    StatusUpdate {
        /// Full order state at this point in time.
        order: OrderResponse,
    },

    /// A server-side error that does **not** close the connection by
    /// itself.  The server may still send a close frame afterwards.
    Error {
        /// Application-level error code (mirrors [`WsCloseCode`] values
        /// where applicable).
        code: u16,
        /// Human-readable reason.
        reason: String,
    },
}

/// Well-known WebSocket close codes used by the order status stream.
///
/// Codes in the 4000–4999 range are reserved for application use by
/// [RFC 6455 §7.4.2](https://www.rfc-editor.org/rfc/rfc6455#section-7.4.2).
pub struct WsCloseCode;

impl WsCloseCode {
    /// Normal closure after a terminal order status has been delivered.
    pub const NORMAL: u16 = 1000;

    /// An unexpected server-side error prevented the connection from
    /// continuing.
    pub const INTERNAL_ERROR: u16 = 1011;

    /// The requested order does not exist.
    pub const ORDER_NOT_FOUND: u16 = 4004;
}
