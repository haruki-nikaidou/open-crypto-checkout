use axum::{
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use kanau::processor::Processor;
use ocrch_core::entities::order_records::{GetOrderRecordById, OrderStatus};
use ocrch_core::framework::DatabaseProcessor;
use ocrch_sdk::objects::ws::{WsCloseCode, WsServerMessage};
use uuid::Uuid;

use super::to_response;
use crate::api::extractors::VerifiedUrl;
use crate::state::AppState;

/// `GET /orders/{order_id}/ws` — WebSocket order status stream.
///
/// Upgrades the HTTP connection to a WebSocket and pushes
/// [`OrderResponse`] JSON frames whenever the order status changes.
/// The first frame is always the current status; the connection is
/// closed after a terminal status (`Paid`, `Expired`, `Cancelled`).
pub(super) async fn order_status_ws(
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
            let _ = send_json(
                &mut socket,
                &WsServerMessage::Error {
                    code: WsCloseCode::ORDER_NOT_FOUND,
                    reason: "order not found".into(),
                },
            )
            .await;
            let _ = socket
                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: WsCloseCode::ORDER_NOT_FOUND,
                    reason: "order not found".into(),
                })))
                .await;
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, %order_id, "WS: failed to query order");
            let _ = send_json(
                &mut socket,
                &WsServerMessage::Error {
                    code: WsCloseCode::INTERNAL_ERROR,
                    reason: "internal error".into(),
                },
            )
            .await;
            let _ = socket
                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: WsCloseCode::INTERNAL_ERROR,
                    reason: "internal error".into(),
                })))
                .await;
            return;
        }
    };

    let msg = WsServerMessage::StatusUpdate {
        order: to_response(&record),
    };
    if send_json(&mut socket, &msg).await.is_err() {
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
            result = broadcast_rx.recv() => {
                match result {
                    Ok(update) if update.order_id == order_id => {
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

                        let msg = WsServerMessage::StatusUpdate {
                            order: to_response(&record),
                        };
                        if send_json(&mut socket, &msg).await.is_err() {
                            return;
                        }

                        if is_terminal(record.status) {
                            let _ = socket.send(Message::Close(None)).await;
                            return;
                        }
                    }
                    Ok(_) => {
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            %order_id,
                            skipped = n,
                            "WS: broadcast receiver lagged, checking current status"
                        );
                        let record = match processor
                            .process(GetOrderRecordById { order_id })
                            .await
                        {
                            Ok(Some(r)) => r,
                            Ok(None) => break,
                            Err(_) => break,
                        };

                        let msg = WsServerMessage::StatusUpdate {
                            order: to_response(&record),
                        };
                        if send_json(&mut socket, &msg).await.is_err() {
                            return;
                        }
                        if is_terminal(record.status) {
                            let _ = socket.send(Message::Close(None)).await;
                            return;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }

            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => {
                        return;
                    }
                    Some(Ok(_)) => {
                    }
                    Some(Err(_)) => {
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
