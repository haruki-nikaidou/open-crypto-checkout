//! Wire types shared across all Ocrch APIs.
//!
//! Structs and enums in this module are serialized to / deserialized from JSON
//! on the wire.  They are grouped by concern:
//!
//! - [`blockchains`] – supported blockchains and stablecoins.
//! - [`create_payment`] – order creation / status types (Service API).
//! - [`user`] – checkout frontend types (User API).
//! - [`webhook`] – webhook payload types.
//! - [`ws`] – WebSocket message types.
//! - [`admin`] – admin dashboard types (Admin API).

pub mod admin;
/// Supported blockchains and stablecoins with their on-chain contract addresses.
pub mod blockchains;
/// Order creation and status types used by the Service API.
pub mod create_payment;
pub mod user;
pub mod webhook;
pub mod ws;

pub use blockchains::{Blockchain, Stablecoin};
pub use create_payment::{GetOrderRequest, OrderResponse, PaymentCreatingEssential};
pub use user::{ChainCoinPair, PaymentDetail, SelectPaymentMethod};
pub use webhook::{OrderStatus, OrderStatusChangedPayload, TransferStatus, UnknownTransferPayload};
pub use ws::{WsCloseCode, WsServerMessage};

pub use crate::signature::{Signature, SignatureError, SignedObject};
