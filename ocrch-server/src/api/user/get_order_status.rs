use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
};
use kanau::processor::Processor;
use ocrch_core::entities::order_records::GetOrderRecordById;
use ocrch_core::framework::DatabaseProcessor;
use uuid::Uuid;

use super::{UserApiError, to_response};
use crate::api::extractors::VerifiedUrl;
use crate::state::AppState;

/// `GET /orders/{order_id}/status` — poll order status.
///
/// Returns the current status of the order.
pub(super) async fn get_order_status(
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

    Ok(Json(to_response(&record)))
}
