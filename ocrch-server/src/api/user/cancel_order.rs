use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
};
use kanau::processor::Processor;
use ocrch_core::entities::erc20_pending_deposit::Erc20PendingDeposit;
use ocrch_core::entities::order_records::{GetOrderRecordById, OrderRecord, OrderStatus};
use ocrch_core::entities::trc20_pending_deposit::Trc20PendingDeposit;
use ocrch_core::events::WebhookEvent;
use ocrch_core::framework::DatabaseProcessor;
use uuid::Uuid;

use super::{UserApiError, to_response};
use crate::api::extractors::VerifiedUrl;
use crate::state::{AppState, OrderStatusUpdate};

/// `POST /orders/{order_id}/cancel` — cancel a pending order.
///
/// Sets the order status to `Cancelled`, deletes all pending deposits
/// (both ERC-20 and TRC-20), and emits a webhook event.
pub(super) async fn cancel_order(
    state: State<AppState>,
    _verified: VerifiedUrl,
    Path(order_id): Path<Uuid>,
) -> Result<impl IntoResponse, UserApiError> {
    let processor = DatabaseProcessor {
        pool: state.db.clone(),
    };

    let record = processor
        .process(GetOrderRecordById { order_id })
        .await
        .map_err(UserApiError::Database)?
        .ok_or(UserApiError::NotFound)?;

    if record.status != OrderStatus::Pending {
        return Err(UserApiError::OrderNotPending);
    }

    let mut tx = state.db.begin().await.map_err(UserApiError::Database)?;

    OrderRecord::update_status_tx(&mut tx, order_id, OrderStatus::Cancelled)
        .await
        .map_err(UserApiError::Database)?;

    Erc20PendingDeposit::delete_for_order_tx(&mut tx, order_id)
        .await
        .map_err(UserApiError::Database)?;

    Trc20PendingDeposit::delete_for_order_tx(&mut tx, order_id)
        .await
        .map_err(UserApiError::Database)?;

    tx.commit().await.map_err(UserApiError::Database)?;

    let webhook_event = WebhookEvent::OrderStatusChanged {
        order_id,
        new_status: OrderStatus::Cancelled,
    };
    if let Err(e) = state.event_senders.webhook_event.send(webhook_event).await {
        tracing::error!(error = %e, "Failed to emit OrderStatusChanged webhook event");
    }

    let _ = state.order_status_tx.send(OrderStatusUpdate {
        order_id,
        new_status: OrderStatus::Cancelled,
    });

    let updated = processor
        .process(GetOrderRecordById { order_id })
        .await
        .map_err(UserApiError::Database)?
        .ok_or(UserApiError::NotFound)?;

    Ok(Json(to_response(&updated)))
}
