//! Service API handlers.
//!
//! These endpoints are called by the application backend and require
//! a signed body verified via the `Ocrch-Signature` header.
//!
//! # Endpoints
//!
//! - `POST /orders`       – create a new pending order
//! - `POST /orders/status` – get the status of an existing order

use axum::{Json, Router, http::StatusCode, response::IntoResponse, routing::post};
use kanau::processor::Processor;
use ocrch_core::entities::order_records::{CreateOrderRecord, GetOrderRecordById, OrderRecord};
use ocrch_core::framework::DatabaseProcessor;
use ocrch_sdk::objects::{GetOrderRequest, OrderResponse, PaymentCreatingEssential};

use crate::api::extractors::SignedBody;
use crate::state::AppState;

/// Build the Service API router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/orders", post(create_order))
        .route("/orders/status", post(get_order_status))
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

/// `POST /orders` — create a new pending order.
///
/// Accepts a signed `PaymentCreatingEssential` body and inserts a new
/// order record into the database with status `pending`.
async fn create_order(
    state: axum::extract::State<AppState>,
    SignedBody(payload): SignedBody<PaymentCreatingEssential>,
) -> Result<impl IntoResponse, ServiceApiError> {
    let processor = DatabaseProcessor {
        pool: state.db.clone(),
    };

    let record = processor
        .process(CreateOrderRecord {
            merchant_order_id: payload.order_id,
            amount: payload.amount,
            webhook_url: payload.webhook_url,
        })
        .await
        .map_err(ServiceApiError::Database)?;

    Ok((StatusCode::CREATED, Json(to_response(&record))))
}

/// `POST /orders/status` — get the status of an existing order.
///
/// Accepts a signed `GetOrderRequest` body containing the order UUID.
async fn get_order_status(
    state: axum::extract::State<AppState>,
    SignedBody(payload): SignedBody<GetOrderRequest>,
) -> Result<impl IntoResponse, ServiceApiError> {
    let processor = DatabaseProcessor {
        pool: state.db.clone(),
    };

    let record = processor
        .process(GetOrderRecordById {
            order_id: payload.order_id,
        })
        .await
        .map_err(ServiceApiError::Database)?
        .ok_or(ServiceApiError::NotFound)?;

    Ok(Json(to_response(&record)))
}

/// Errors that can occur in Service API handlers.
#[derive(Debug)]
enum ServiceApiError {
    /// A database query failed.
    Database(sqlx::Error),
    /// The requested order was not found.
    NotFound,
}

impl IntoResponse for ServiceApiError {
    fn into_response(self) -> axum::response::Response {
        match self {
            ServiceApiError::Database(e) => {
                tracing::error!(error = %e, "Service API database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response()
            }
            ServiceApiError::NotFound => (StatusCode::NOT_FOUND, "order not found").into_response(),
        }
    }
}
