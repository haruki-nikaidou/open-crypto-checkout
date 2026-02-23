use axum::{extract::Path, http::StatusCode, response::IntoResponse};
use kanau::processor::Processor;
use ocrch_core::entities::order_records::GetOrderRecordById;
use ocrch_core::events::WebhookEvent;
use ocrch_core::framework::DatabaseProcessor;
use uuid::Uuid;

use crate::api::extractors::AdminAuth;
use crate::state::AppState;

use super::AdminApiError;

/// `POST /orders/{order_id}/resend-webhook` — resend the order status webhook.
///
/// Emits a `WebhookEvent::OrderStatusChanged` for the order's current status,
/// causing the webhook sender to re-deliver it.
pub async fn resend_order_webhook(
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

    state
        .event_senders
        .webhook_event
        .send(WebhookEvent::OrderStatusChanged {
            order_id,
            new_status: order.status,
        })
        .await
        .map_err(|_| AdminApiError::EventChannelClosed)?;

    Ok(StatusCode::ACCEPTED)
}
