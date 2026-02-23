//! Admin API handlers.
//!
//! These endpoints are called by the admin dashboard frontend and require
//! the `Ocrch-Admin-Authorization` header with the plaintext admin secret.
//!
//! # Endpoints
//!
//! - `GET  /orders`                           – list orders (paginated, filterable)
//! - `GET  /deposits`                         – list pending deposits (paginated, filterable)
//! - `GET  /wallets/{address}/transfers`      – list transfers for a wallet
//! - `GET  /wallets`                          – show wallets and enabled coins
//! - `POST /orders/{order_id}/mark-paid`      – force-mark an order as paid
//! - `POST /orders/{order_id}/resend-webhook` – resend order status webhook
//! - `POST /transfers/{transfer_id}/resend-webhook` – resend unknown transfer webhook

use axum::{
    Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};

use crate::state::AppState;

mod list_deposits;
mod list_orders;
mod list_transfers;
mod mark_paid;
mod resend_order_webhook;
mod resend_transfer_webhook;
mod show_wallets;

/// Build the Admin API router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/orders", get(list_orders::list_orders))
        .route("/deposits", get(list_deposits::list_deposits))
        .route(
            "/wallets/{address}/transfers",
            get(list_transfers::list_transfers),
        )
        .route("/wallets", get(show_wallets::show_wallets))
        .route(
            "/orders/{order_id}/mark-paid",
            post(mark_paid::mark_paid),
        )
        .route(
            "/orders/{order_id}/resend-webhook",
            post(resend_order_webhook::resend_order_webhook),
        )
        .route(
            "/transfers/{transfer_id}/resend-webhook",
            post(resend_transfer_webhook::resend_transfer_webhook),
        )
}

// ---------------------------------------------------------------------------
// Shared error type
// ---------------------------------------------------------------------------

/// Errors that can occur in Admin API handlers.
#[derive(Debug)]
pub(crate) enum AdminApiError {
    Database(sqlx::Error),
    NotFound,
    EventChannelClosed,
}

impl IntoResponse for AdminApiError {
    fn into_response(self) -> axum::response::Response {
        match self {
            AdminApiError::Database(e) => {
                tracing::error!(error = %e, "Admin API database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response()
            }
            AdminApiError::NotFound => {
                (StatusCode::NOT_FOUND, "resource not found").into_response()
            }
            AdminApiError::EventChannelClosed => {
                tracing::error!("Admin API: event channel closed");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

use ocrch_core::entities::order_records::OrderRecord;
use ocrch_sdk::objects::admin::AdminOrderResponse;

pub(crate) fn order_to_admin_response(r: &OrderRecord) -> AdminOrderResponse {
    AdminOrderResponse {
        order_id: r.order_id,
        merchant_order_id: r.merchant_order_id.clone(),
        amount: r.amount,
        status: r.status.into(),
        created_at: r.created_at.assume_utc().unix_timestamp(),
        webhook_url: r.webhook_url.clone(),
        webhook_retry_count: r.webhook_retry_count,
        webhook_success_at: r.webhook_success_at.map(|t| t.assume_utc().unix_timestamp()),
        webhook_last_tried_at: r.webhook_last_tried_at.map(|t| t.assume_utc().unix_timestamp()),
    }
}
