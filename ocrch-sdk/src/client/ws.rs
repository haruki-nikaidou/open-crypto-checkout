//! WebSocket client for the order status stream.

use futures_util::StreamExt;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};

use super::ClientError;
use crate::objects::ws::WsServerMessage;

/// A live WebSocket connection to the order status stream.
///
/// Obtained via [`UserClient::connect_order_status`].  Each call to
/// [`next`](Self::next) waits for the next server-to-client application
/// message; control frames (`Ping`, `Pong`) are handled automatically by
/// tungstenite and are invisible to callers.
///
/// The server closes the connection (returning `None` from `next`) after it
/// has delivered a terminal order status (`Paid`, `Expired`, or `Cancelled`),
/// or immediately when the order is not found / an internal error occurs.
pub struct OrderStatusStream {
    inner: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl OrderStatusStream {
    pub(super) fn new(inner: WebSocketStream<MaybeTlsStream<TcpStream>>) -> Self {
        Self { inner }
    }

    /// Wait for the next message from the server.
    ///
    /// Returns:
    /// - `Some(Ok(msg))` — a deserialized [`WsServerMessage`].
    /// - `Some(Err(e))` — a transport or JSON parse error.
    /// - `None` — the server sent a close frame or the connection was dropped.
    pub async fn next(&mut self) -> Option<Result<WsServerMessage, ClientError>> {
        loop {
            match self.inner.next().await? {
                Ok(Message::Text(text)) => {
                    return Some(serde_json::from_str(&text).map_err(ClientError::Json));
                }
                Ok(Message::Binary(data)) => {
                    return Some(serde_json::from_slice(&data).map_err(ClientError::Json));
                }
                Ok(Message::Close(_)) => return None,
                Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => continue,
                Err(e) => return Some(Err(ClientError::Ws(e))),
            }
        }
    }

    /// Send a graceful WebSocket close frame and flush the connection.
    pub async fn close(&mut self) -> Result<(), ClientError> {
        self.inner.close(None).await.map_err(ClientError::Ws)
    }
}
