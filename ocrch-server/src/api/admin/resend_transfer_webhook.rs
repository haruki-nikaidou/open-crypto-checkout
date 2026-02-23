use axum::{extract::Path, http::StatusCode, response::IntoResponse};
use kanau::processor::Processor;
use ocrch_core::entities::erc20_transfer::GetErc20TransferById;
use ocrch_core::entities::trc20_transfer::GetTrc20TransferById;
use ocrch_core::events::{BlockchainTarget, WebhookEvent};
use ocrch_core::framework::DatabaseProcessor;

use crate::api::extractors::AdminAuth;
use crate::state::AppState;

use super::AdminApiError;

/// `POST /transfers/{transfer_id}/resend-webhook` — resend an unknown transfer webhook.
///
/// Looks up the transfer by ID in both ERC-20 and TRC-20 tables, then emits a
/// `WebhookEvent::UnknownTransferReceived` event.
pub async fn resend_transfer_webhook(
    state: axum::extract::State<AppState>,
    _auth: AdminAuth,
    Path(transfer_id): Path<i64>,
) -> Result<impl IntoResponse, AdminApiError> {
    let processor = DatabaseProcessor {
        pool: state.db.clone(),
    };

    if let Some(erc20) = processor
        .process(GetErc20TransferById { id: transfer_id })
        .await
        .map_err(AdminApiError::Database)?
    {
        state
            .event_senders
            .webhook_event
            .send(WebhookEvent::UnknownTransferReceived {
                transfer_id,
                blockchain: BlockchainTarget::Erc20(erc20.chain),
            })
            .await
            .map_err(|_| AdminApiError::EventChannelClosed)?;

        return Ok(StatusCode::ACCEPTED);
    }

    if processor
        .process(GetTrc20TransferById { id: transfer_id })
        .await
        .map_err(AdminApiError::Database)?
        .is_some()
    {
        state
            .event_senders
            .webhook_event
            .send(WebhookEvent::UnknownTransferReceived {
                transfer_id,
                blockchain: BlockchainTarget::Trc20,
            })
            .await
            .map_err(|_| AdminApiError::EventChannelClosed)?;

        return Ok(StatusCode::ACCEPTED);
    }

    Err(AdminApiError::NotFound)
}
