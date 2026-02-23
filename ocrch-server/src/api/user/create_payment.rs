use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use kanau::processor::Processor;
use ocrch_core::entities::StablecoinName;
use ocrch_core::entities::erc20_pending_deposit::Erc20PendingDepositInsert;
use ocrch_core::entities::order_records::{GetOrderRecordById, OrderStatus};
use ocrch_core::entities::trc20_pending_deposit::Trc20PendingDepositInsert;
use ocrch_core::events::PendingDepositChanged;
use ocrch_core::framework::DatabaseProcessor;
use ocrch_sdk::objects::blockchains::Blockchain;
use ocrch_sdk::objects::{PaymentDetail, SelectPaymentMethod};
use uuid::Uuid;

use super::{UserApiError, blockchain_to_etherscan_chain};
use crate::api::extractors::VerifiedUrl;
use crate::state::AppState;

/// `POST /orders/{order_id}/payment` — select a payment method.
///
/// Creates a new pending deposit for the given order on the selected
/// blockchain and stablecoin, then emits a `PendingDepositChanged` event
/// so the pooling pipeline begins watching for the payment.
pub(super) async fn create_payment(
    state: State<AppState>,
    _verified: VerifiedUrl,
    Path(order_id): Path<Uuid>,
    Json(body): Json<SelectPaymentMethod>,
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

    let wallets = state.config.wallets.read().await;
    let wallet = wallets
        .iter()
        .find(|w| w.blockchain == body.blockchain && w.enabled_coins.contains(&body.stablecoin))
        .ok_or(UserApiError::WalletNotFound)?;

    let wallet_address = wallet.address.clone();
    let blockchain = wallet.blockchain;
    drop(wallets);

    let token: StablecoinName = body.stablecoin.into();

    let event = match blockchain {
        Blockchain::Tron => {
            let deposit = processor
                .process(Trc20PendingDepositInsert {
                    order: order_id,
                    token_name: token,
                    user_address: None,
                    wallet_address: wallet_address.clone(),
                    value: record.amount,
                })
                .await
                .map_err(UserApiError::Database)?;

            PendingDepositChanged::Trc20 {
                deposit_id: deposit.id,
                token,
            }
        }
        other => {
            let chain = blockchain_to_etherscan_chain(other)?;
            let deposit = processor
                .process(Erc20PendingDepositInsert {
                    order: order_id,
                    token_name: token,
                    chain,
                    user_address: None,
                    wallet_address: wallet_address.clone(),
                    value: record.amount,
                })
                .await
                .map_err(UserApiError::Database)?;

            PendingDepositChanged::Erc20 {
                deposit_id: deposit.id,
                chain,
                token,
            }
        }
    };

    if let Err(e) = state
        .event_senders
        .pending_deposit_changed
        .send(event)
        .await
    {
        tracing::error!(error = %e, "Failed to emit PendingDepositChanged event");
    }

    Ok((
        StatusCode::CREATED,
        Json(PaymentDetail {
            order_id,
            wallet_address,
            amount: record.amount,
            blockchain: body.blockchain,
            stablecoin: body.stablecoin,
        }),
    ))
}
