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
    Json, Router,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use kanau::processor::Processor;
use ocrch_core::entities::StablecoinName;
use ocrch_core::entities::erc20_pending_deposit::{
    Erc20PendingDeposit, Erc20PendingDepositInsert, EtherScanChain,
};
use ocrch_core::entities::order_records::{GetOrderRecordById, OrderRecord, OrderStatus};
use ocrch_core::entities::trc20_pending_deposit::{Trc20PendingDeposit, Trc20PendingDepositInsert};
use ocrch_core::events::{PendingDepositChanged, WebhookEvent};
use ocrch_core::framework::DatabaseProcessor;
use ocrch_sdk::objects::blockchains::Blockchain;
use ocrch_sdk::objects::{ChainCoinPair, OrderResponse, PaymentDetail, SelectPaymentMethod};
use uuid::Uuid;

use crate::api::extractors::VerifiedUrl;
use crate::state::{AppState, OrderStatusUpdate};

/// Build the User API router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/chains", get(get_chains))
        .route("/orders/{order_id}/payment", post(create_payment))
        .route("/orders/{order_id}/cancel", post(cancel_order))
        .route("/orders/{order_id}/status", get(get_order_status))
        .route("/orders/{order_id}/ws", get(order_status_ws))
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
// GET /chains
// ---------------------------------------------------------------------------

/// `GET /chains` — list available blockchain + stablecoin payment options.
///
/// Returns every (blockchain, stablecoin, wallet_address) triple derived
/// from the configured wallets.
async fn get_chains(
    state: State<AppState>,
    _verified: VerifiedUrl,
) -> Result<impl IntoResponse, UserApiError> {
    let wallets = state.config.wallets.read().await;
    let pairs: Vec<ChainCoinPair> = wallets
        .iter()
        .flat_map(|w| {
            w.enabled_coins.iter().map(move |coin| ChainCoinPair {
                blockchain: w.blockchain,
                stablecoin: *coin,
                wallet_address: w.address.clone(),
            })
        })
        .collect();
    drop(wallets);
    Ok(Json(pairs))
}

// ---------------------------------------------------------------------------
// POST /orders/{order_id}/payment
// ---------------------------------------------------------------------------

/// `POST /orders/{order_id}/payment` — select a payment method.
///
/// Creates a new pending deposit for the given order on the selected
/// blockchain and stablecoin, then emits a `PendingDepositChanged` event
/// so the pooling pipeline begins watching for the payment.
async fn create_payment(
    state: State<AppState>,
    _verified: VerifiedUrl,
    Path(order_id): Path<Uuid>,
    Json(body): Json<SelectPaymentMethod>,
) -> Result<impl IntoResponse, UserApiError> {
    let processor = DatabaseProcessor {
        pool: state.db.clone(),
    };

    // 1. Validate order exists and is pending
    let record = processor
        .process(GetOrderRecordById { order_id })
        .await
        .map_err(UserApiError::Database)?
        .ok_or(UserApiError::NotFound)?;

    if record.status != OrderStatus::Pending {
        return Err(UserApiError::OrderNotPending);
    }

    // 2. Find matching wallet in config
    let wallets = state.config.wallets.read().await;
    let wallet = wallets
        .iter()
        .find(|w| w.blockchain == body.blockchain && w.enabled_coins.contains(&body.stablecoin))
        .ok_or(UserApiError::WalletNotFound)?;

    let wallet_address = wallet.address.clone();
    let blockchain = wallet.blockchain;
    drop(wallets);

    let token: StablecoinName = body.stablecoin.into();

    // 3. Create pending deposit (ERC-20 or TRC-20)
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

    // 4. Emit PendingDepositChanged event
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

// ---------------------------------------------------------------------------
// POST /orders/{order_id}/cancel
// ---------------------------------------------------------------------------

/// `POST /orders/{order_id}/cancel` — cancel a pending order.
///
/// Sets the order status to `Cancelled`, deletes all pending deposits
/// (both ERC-20 and TRC-20), and emits a webhook event.
async fn cancel_order(
    state: State<AppState>,
    _verified: VerifiedUrl,
    Path(order_id): Path<Uuid>,
) -> Result<impl IntoResponse, UserApiError> {
    let processor = DatabaseProcessor {
        pool: state.db.clone(),
    };

    // 1. Validate order exists and is pending
    let record = processor
        .process(GetOrderRecordById { order_id })
        .await
        .map_err(UserApiError::Database)?
        .ok_or(UserApiError::NotFound)?;

    if record.status != OrderStatus::Pending {
        return Err(UserApiError::OrderNotPending);
    }

    // 2. In a transaction: update status + delete all pending deposits
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

    // 3. Emit webhook event
    let webhook_event = WebhookEvent::OrderStatusChanged {
        order_id,
        new_status: OrderStatus::Cancelled,
    };
    if let Err(e) = state.event_senders.webhook_event.send(webhook_event).await {
        tracing::error!(error = %e, "Failed to emit OrderStatusChanged webhook event");
    }

    // 3b. Broadcast to WebSocket clients
    let _ = state.order_status_tx.send(OrderStatusUpdate {
        order_id,
        new_status: OrderStatus::Cancelled,
    });

    // 4. Return updated order (re-read to get consistent state)
    let updated = processor
        .process(GetOrderRecordById { order_id })
        .await
        .map_err(UserApiError::Database)?
        .ok_or(UserApiError::NotFound)?;

    Ok(Json(to_response(&updated)))
}

// ---------------------------------------------------------------------------
// GET /orders/{order_id}/status
// ---------------------------------------------------------------------------

/// `GET /orders/{order_id}/status` — poll order status.
///
/// Returns the current status of the order.
async fn get_order_status(
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

// ---------------------------------------------------------------------------
// GET /orders/{order_id}/ws
// ---------------------------------------------------------------------------

/// `GET /orders/{order_id}/ws` — WebSocket order status stream.
///
/// Upgrades the HTTP connection to a WebSocket and pushes
/// [`OrderResponse`] JSON frames whenever the order status changes.
/// The first frame is always the current status; the connection is
/// closed after a terminal status (`Paid`, `Expired`, `Cancelled`).
async fn order_status_ws(
    state: State<AppState>,
    _verified: VerifiedUrl,
    Path(order_id): Path<Uuid>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let app_state = state.0.clone();
    ws.on_upgrade(move |socket| handle_order_ws(socket, app_state, order_id))
}

/// Returns `true` if the given status is a terminal (final) state.
fn is_terminal(status: OrderStatus) -> bool {
    matches!(
        status,
        OrderStatus::Paid | OrderStatus::Expired | OrderStatus::Cancelled
    )
}

/// Background task that drives a single WebSocket connection.
///
/// 1. Sends the current order status as the first message.
/// 2. If already terminal, closes immediately.
/// 3. Otherwise subscribes to the broadcast channel and forwards
///    status updates for this `order_id` until a terminal state is
///    reached or the client disconnects.
async fn handle_order_ws(mut socket: WebSocket, state: AppState, order_id: Uuid) {
    let processor = DatabaseProcessor {
        pool: state.db.clone(),
    };

    // Subscribe to the broadcast channel *before* reading the current
    // status so that any update that races with our DB query is still
    // captured in the receiver's buffer.
    let mut broadcast_rx = state.order_status_tx.subscribe();

    // --- Send current status as the first message --------------------------
    let record = match processor.process(GetOrderRecordById { order_id }).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            let _ = socket
                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: 4004,
                    reason: "order not found".into(),
                })))
                .await;
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, %order_id, "WS: failed to query order");
            let _ = socket
                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: 1011,
                    reason: "internal error".into(),
                })))
                .await;
            return;
        }
    };

    let response = to_response(&record);
    if send_json(&mut socket, &response).await.is_err() {
        return;
    }

    // If already terminal, close after the first message
    if is_terminal(record.status) {
        let _ = socket.send(Message::Close(None)).await;
        return;
    }

    // --- Relay updates until terminal or disconnect ------------------------

    loop {
        tokio::select! {
            // Incoming broadcast event
            result = broadcast_rx.recv() => {
                match result {
                    Ok(update) if update.order_id == order_id => {
                        // Re-read from DB for a consistent OrderResponse
                        let record = match processor
                            .process(GetOrderRecordById { order_id })
                            .await
                        {
                            Ok(Some(r)) => r,
                            Ok(None) => break,
                            Err(e) => {
                                tracing::error!(
                                    error = %e,
                                    %order_id,
                                    "WS: failed to query order on update"
                                );
                                break;
                            }
                        };

                        let response = to_response(&record);
                        if send_json(&mut socket, &response).await.is_err() {
                            return; // client gone
                        }

                        if is_terminal(record.status) {
                            let _ = socket.send(Message::Close(None)).await;
                            return;
                        }
                    }
                    Ok(_) => {
                        // Update for a different order — ignore
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            %order_id,
                            skipped = n,
                            "WS: broadcast receiver lagged, checking current status"
                        );
                        // After lag, re-read current status in case we missed ours
                        let record = match processor
                            .process(GetOrderRecordById { order_id })
                            .await
                        {
                            Ok(Some(r)) => r,
                            Ok(None) => break,
                            Err(_) => break,
                        };

                        let response = to_response(&record);
                        if send_json(&mut socket, &response).await.is_err() {
                            return;
                        }
                        if is_terminal(record.status) {
                            let _ = socket.send(Message::Close(None)).await;
                            return;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        // Broadcast channel closed (server shutting down)
                        break;
                    }
                }
            }

            // Incoming WebSocket frame from the client (ping/pong/close)
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => {
                        // Client disconnected
                        return;
                    }
                    Some(Ok(_)) => {
                        // Ignore other client messages (text, binary, ping)
                    }
                    Some(Err(_)) => {
                        // WebSocket error — drop the connection
                        return;
                    }
                }
            }
        }
    }

    let _ = socket.send(Message::Close(None)).await;
}

/// Serialize `value` as JSON and send it as a text WebSocket frame.
///
/// Returns `Err(())` if the send fails (client disconnected).
async fn send_json<T: serde::Serialize>(socket: &mut WebSocket, value: &T) -> Result<(), ()> {
    let json = serde_json::to_string(value).map_err(|_| ())?;
    socket
        .send(Message::Text(json.into()))
        .await
        .map_err(|_| ())
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
