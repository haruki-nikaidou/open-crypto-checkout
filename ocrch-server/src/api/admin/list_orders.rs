use axum::{Json, extract::Query, response::IntoResponse};
use kanau::processor::Processor;
use ocrch_core::entities::order_records::ListOrderRecords;
use ocrch_core::framework::DatabaseProcessor;
use ocrch_sdk::objects::admin::{ListOrdersQuery, clamp_pagination};

use crate::api::extractors::AdminAuth;
use crate::state::AppState;

use super::{AdminApiError, order_to_admin_response};

/// `GET /orders` — list orders with pagination and optional filters.
pub async fn list_orders(
    state: axum::extract::State<AppState>,
    _auth: AdminAuth,
    Query(query): Query<ListOrdersQuery>,
) -> Result<impl IntoResponse, AdminApiError> {
    let processor = DatabaseProcessor {
        pool: state.db.clone(),
    };

    let (limit, offset) = clamp_pagination(query.limit, query.offset);

    let records = processor
        .process(ListOrderRecords {
            limit,
            offset,
            status: query.status.map(Into::into),
            merchant_order_id: query.merchant_order_id,
        })
        .await
        .map_err(AdminApiError::Database)?;

    let response: Vec<_> = records.iter().map(order_to_admin_response).collect();
    Ok(Json(response))
}
