//! User API handlers.
//!
//! These endpoints are called by the checkout frontend (user's browser)
//! and require a verified signed frontend URL via the `Ocrch-Signature`
//! and `Ocrch-Signed-Url` headers.
//!
//! # Endpoints
//!
//! - `GET  /chains`                    – list available chain-coin pairs
//! - `POST /orders/{order_id}/payment` – select payment method / create pending deposit
//! - `POST /orders/{order_id}/cancel`  – cancel order
//! - `GET  /orders/{order_id}/status`  – poll order status
//! - `GET  /orders/{order_id}/ws`      – WebSocket order status stream

use axum::{
    Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use ocrch_core::entities::erc20_pending_deposit::EtherScanChain;
use ocrch_core::entities::order_records::OrderRecord;
use ocrch_sdk::objects::OrderResponse;
use ocrch_sdk::objects::blockchains::Blockchain;

use crate::state::AppState;

mod cancel_order;
mod chains;
mod create_payment;
mod get_order_status;
mod ws;

/// Build the User API router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/chains", get(chains::get_chains))
        .route(
            "/orders/{order_id}/payment",
            post(create_payment::create_payment),
        )
        .route(
            "/orders/{order_id}/cancel",
            post(cancel_order::cancel_order),
        )
        .route(
            "/orders/{order_id}/status",
            get(get_order_status::get_order_status),
        )
        .route("/orders/{order_id}/ws", get(ws::order_status_ws))
}

/// Convert an `OrderRecord` (DB model) into an `OrderResponse` (API model).
fn to_response(record: &OrderRecord) -> OrderResponse {
    OrderResponse {
        order_id: record.order_id,
        merchant_order_id: record.merchant_order_id.clone(),
        amount: record.amount,
        status: record.status.into(),
        created_at: record.created_at.assume_utc().unix_timestamp(),
    }
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

/// Errors that can occur in User API handlers.
#[derive(Debug)]
enum UserApiError {
    /// A database query failed.
    Database(sqlx::Error),
    /// The requested order was not found.
    NotFound,
    /// The order is not in a pending state.
    OrderNotPending,
    /// No wallet matches the selected blockchain + stablecoin.
    WalletNotFound,
    /// The selected blockchain is invalid (e.g. Tron passed to EtherScan).
    InvalidChain,
}

impl IntoResponse for UserApiError {
    fn into_response(self) -> axum::response::Response {
        match self {
            UserApiError::Database(e) => {
                tracing::error!(error = %e, "User API database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response()
            }
            UserApiError::NotFound => (StatusCode::NOT_FOUND, "order not found").into_response(),
            UserApiError::OrderNotPending => {
                (StatusCode::CONFLICT, "order is not pending").into_response()
            }
            UserApiError::WalletNotFound => (
                StatusCode::BAD_REQUEST,
                "no wallet available for the selected chain and coin",
            )
                .into_response(),
            UserApiError::InvalidChain => {
                (StatusCode::BAD_REQUEST, "invalid blockchain selection").into_response()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map an SDK `Blockchain` variant to an `EtherScanChain`.
///
/// Returns `Err(UserApiError::InvalidChain)` if called with `Blockchain::Tron`.
fn blockchain_to_etherscan_chain(blockchain: Blockchain) -> Result<EtherScanChain, UserApiError> {
    match blockchain {
        Blockchain::Ethereum => Ok(EtherScanChain::Ethereum),
        Blockchain::Polygon => Ok(EtherScanChain::Polygon),
        Blockchain::Base => Ok(EtherScanChain::Base),
        Blockchain::ArbitrumOne => Ok(EtherScanChain::ArbitrumOne),
        Blockchain::Linea => Ok(EtherScanChain::Linea),
        Blockchain::Optimism => Ok(EtherScanChain::Optimism),
        Blockchain::AvalancheC => Ok(EtherScanChain::AvalancheC),
        Blockchain::Tron => Err(UserApiError::InvalidChain),
    }
}
