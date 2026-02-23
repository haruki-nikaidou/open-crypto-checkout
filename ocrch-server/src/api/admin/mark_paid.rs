use axum::{Json, extract::Path, response::IntoResponse};
use kanau::processor::Processor;
use ocrch_core::entities::order_records::{GetOrderRecordById, OrderStatus, UpdateOrderStatus};
use ocrch_core::events::WebhookEvent;
use ocrch_core::framework::DatabaseProcessor;
use uuid::Uuid;

use crate::api::extractors::AdminAuth;
use crate::state::{AppState, OrderStatusUpdate};

use super::{AdminApiError, order_to_admin_response};

/// `POST /orders/{order_id}/mark-paid` — force-mark an order as paid.
///
/// Updates the order status to `Paid`, emits a webhook event, and broadcasts
/// to WebSocket clients.
pub async fn mark_paid(
    state: axum::extract::State<AppState>,
    _auth: AdminAuth,
    Path(order_id): Path<Uuid>,
) -> Result<impl IntoResponse, AdminApiError> {
    let processor = DatabaseProcessor {
        pool: state.db.clone(),
    };

    let order = processor
        .process(GetOrderRecordById { order_id })
        .await
        .map_err(AdminApiError::Database)?
        .ok_or(AdminApiError::NotFound)?;

    if order.status == OrderStatus::Paid {
        return Ok(Json(order_to_admin_response(&order)));
    }

    processor
        .process(UpdateOrderStatus {
            order_id,
            status: OrderStatus::Paid,
        })
        .await
        .map_err(AdminApiError::Database)?;

    state
        .event_senders
        .webhook_event
        .send(WebhookEvent::OrderStatusChanged {
            order_id,
            new_status: OrderStatus::Paid,
        })
        .await
        .map_err(|_| AdminApiError::EventChannelClosed)?;

    let _ = state.order_status_tx.send(OrderStatusUpdate {
        order_id,
        new_status: OrderStatus::Paid,
    });

    let updated = processor
        .process(GetOrderRecordById { order_id })
        .await
        .map_err(AdminApiError::Database)?
        .ok_or(AdminApiError::NotFound)?;

    Ok(Json(order_to_admin_response(&updated)))
}
