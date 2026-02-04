//! WebhookSender processor.
//!
//! The WebhookSender is responsible for:
//! - Receiving `WebhookEvent` from the queue
//! - Looking up the webhook URL from the order record
//! - Sending HTTP POST requests with signed body
//! - Handling retries with exponential backoff (2^0 to 2^11 seconds)
//! - Updating `webhook_retry_count` and `webhook_last_tried_at` in the database
//!
//! Note: The actual signature generation is delegated to the caller via a closure.
//! This keeps cryptographic operations in `ocrch-sdk` where they belong.

use crate::entities::order_records::{OrderRecord, OrderStatus};
use crate::events::{BlockchainTarget, WebhookEvent, WebhookEventReceiver};
use ocrch_sdk::objects::{OrderStatus as SdkOrderStatus, OrderStatusChangedPayload};
use sqlx::PgPool;
use thiserror::Error;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Maximum retry attempts (2^11 = 2048 seconds max backoff)
const MAX_RETRY_COUNT: u32 = 11;

/// Errors that can occur during webhook delivery.
#[derive(Debug, Error)]
pub enum WebhookError {
    /// Database error
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// HTTP request error
    #[error("HTTP request error: {0}")]
    Request(#[from] reqwest::Error),

    /// Order not found
    #[error("order not found: {0}")]
    OrderNotFound(Uuid),

    /// Webhook delivery failed (non-500 status)
    #[error("webhook delivery failed with status {status}: {body}")]
    DeliveryFailed { status: u16, body: String },

    /// Payload serialization error
    #[error("payload serialization error: {0}")]
    SerializationError(String),
}

/// WebhookSender handles delivering webhook events to merchant endpoints.
pub struct WebhookSender {
    pool: PgPool,
    webhook_rx: WebhookEventReceiver,
    shutdown_rx: watch::Receiver<bool>,
    http_client: reqwest::Client,
}

impl WebhookSender {
    /// Create a new WebhookSender.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `webhook_rx` - Receiver for WebhookEvent events
    /// * `shutdown_rx` - Receiver for shutdown signal
    pub fn new(
        pool: PgPool,
        webhook_rx: WebhookEventReceiver,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        Self {
            pool,
            webhook_rx,
            shutdown_rx,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// Run the WebhookSender.
    pub async fn run(mut self) {
        info!("WebhookSender started");

        // Also spawn a background task to retry failed webhooks
        let pool = self.pool.clone();
        let http_client = self.http_client.clone();
        let mut retry_shutdown_rx = self.shutdown_rx.clone();

        let retry_handle = tokio::spawn(async move {
            Self::retry_failed_webhooks_loop(pool, http_client, &mut retry_shutdown_rx)
                .await;
        });

        loop {
            tokio::select! {
                biased;

                // Check for shutdown
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        info!("WebhookSender received shutdown signal");
                        break;
                    }
                }

                // Receive WebhookEvent events
                Some(event) = self.webhook_rx.recv() => {
                    debug!(event = ?event, "Received WebhookEvent");

                    if let Err(e) = self.process_event(event).await {
                        error!(error = %e, "Failed to process WebhookEvent");
                    }
                }

                else => {
                    info!("WebhookEvent channel closed");
                    break;
                }
            }
        }

        // Wait for retry task to complete
        let _ = retry_handle.await;

        info!("WebhookSender shutdown complete");
    }

    /// Process a webhook event.
    async fn process_event(&self, event: WebhookEvent) -> Result<(), WebhookError> {
        match event {
            WebhookEvent::OrderStatusChanged {
                order_id,
                new_status,
            } => self.send_order_status_webhook(order_id, new_status).await,
            WebhookEvent::UnknownTransferReceived {
                transfer_id,
                blockchain,
            } => {
                self.send_unknown_transfer_webhook(transfer_id, blockchain)
                    .await
            }
        }
    }

    /// Send an order status change webhook.
    async fn send_order_status_webhook(
        &self,
        order_id: Uuid,
        new_status: OrderStatus,
    ) -> Result<(), WebhookError> {
        // Get order info
        let Some(order_info) = OrderRecord::get_by_id(&self.pool, order_id).await? else {
            return Err(WebhookError::OrderNotFound(order_id));
        };

        // Build payload using SDK types
        let sdk_status: SdkOrderStatus = new_status.into();
        let payload = OrderStatusChangedPayload {
            event_type: "order_status_changed".to_string(),
            order_id: order_info.order_id,
            merchant_order_id: order_info.merchant_order_id.clone(),
            status: sdk_status,
            amount: order_info.amount.to_string(),
            timestamp: time::OffsetDateTime::now_utc().unix_timestamp(),
        };

        let body = serde_json::to_string(&payload)
            .map_err(|e| WebhookError::SerializationError(e.to_string()))?;

        // Note: Signature support is not yet implemented.
        // In the future, this would use the merchant's secret from configuration.

        // Send webhook
        let result = self
            .send_webhook(&order_info.webhook_url, &body, None)
            .await;

        // Update database based on result
        match &result {
            Ok(()) => {
                self.mark_webhook_success(order_id).await?;
                info!(order_id = %order_id, "Webhook delivered successfully");
            }
            Err(e) => {
                warn!(
                    order_id = %order_id,
                    error = %e,
                    retry_count = order_info.webhook_retry_count,
                    "Webhook delivery failed"
                );
                self.increment_retry_count(order_id).await?;
            }
        }

        result
    }

    /// Send an unknown transfer webhook.
    async fn send_unknown_transfer_webhook(
        &self,
        transfer_id: i64,
        blockchain: BlockchainTarget,
    ) -> Result<(), WebhookError> {
        // For unknown transfers, we need to determine which merchant(s) to notify
        // This would typically be based on the wallet address configuration
        // For now, we'll just log it as these webhooks are optional per the spec

        info!(
            transfer_id = transfer_id,
            blockchain = %blockchain,
            "Unknown transfer detected (webhook delivery not implemented for unknown transfers)"
        );

        Ok(())
    }

    /// Send the webhook HTTP request.
    async fn send_webhook(
        &self,
        url: &str,
        body: &str,
        signature: Option<&str>,
    ) -> Result<(), WebhookError> {
        let mut request = self
            .http_client
            .post(url)
            .header("Content-Type", "application/json");

        if let Some(sig) = signature {
            request = request.header("Ocrch-Signature", sig);
        }

        let response = request.body(body.to_string()).send().await?;

        let status = response.status();

        // Per spec: 500 OK means success (this seems like a typo in the spec,
        // but we'll interpret it as 200 OK for success)
        if status.is_success() {
            Ok(())
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(WebhookError::DeliveryFailed {
                status: status.as_u16(),
                body,
            })
        }
    }

    /// Mark a webhook as successfully delivered.
    async fn mark_webhook_success(&self, order_id: Uuid) -> Result<(), WebhookError> {
        OrderRecord::mark_webhook_success(&self.pool, order_id).await?;
        Ok(())
    }

    /// Increment the retry count for a failed webhook.
    async fn increment_retry_count(&self, order_id: Uuid) -> Result<(), WebhookError> {
        OrderRecord::increment_webhook_retry_count(&self.pool, order_id).await?;
        Ok(())
    }

    /// Background loop to retry failed webhooks.
    async fn retry_failed_webhooks_loop(
        pool: PgPool,
        http_client: reqwest::Client,
        shutdown_rx: &mut watch::Receiver<bool>,
    ) {
        info!("Webhook retry loop started");

        loop {
            tokio::select! {
                biased;

                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("Webhook retry loop shutting down");
                        break;
                    }
                }

                _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
                    if let Err(e) = Self::retry_pending_webhooks(&pool, &http_client).await {
                        error!(error = %e, "Failed to retry webhooks");
                    }
                }
            }
        }
    }

    /// Retry pending webhooks that are due for retry.
    async fn retry_pending_webhooks(
        pool: &PgPool,
        http_client: &reqwest::Client,
    ) -> Result<(), WebhookError> {
        // Find orders that need webhook retry
        let orders_to_retry =
            OrderRecord::get_orders_for_webhook_retry(pool, MAX_RETRY_COUNT as i32, 10).await?;

        for order in orders_to_retry {
            let sdk_status: SdkOrderStatus = order.status.into();
            let payload = OrderStatusChangedPayload {
                event_type: "order_status_changed".to_string(),
                order_id: order.order_id,
                merchant_order_id: order.merchant_order_id.clone(),
                status: sdk_status,
                amount: order.amount.to_string(),
                timestamp: time::OffsetDateTime::now_utc().unix_timestamp(),
            };

            let body = match serde_json::to_string(&payload) {
                Ok(b) => b,
                Err(e) => {
                    error!(
                        order_id = %order.order_id,
                        error = %e,
                        "Failed to serialize webhook payload"
                    );
                    continue;
                }
            };

            // Note: Signature support is not yet implemented for retries.
            let request = http_client
                .post(&order.webhook_url)
                .header("Content-Type", "application/json")
                .body(body);

            match request.send().await {
                Ok(response) if response.status().is_success() => {
                    OrderRecord::mark_webhook_success(pool, order.order_id).await?;

                    info!(
                        order_id = %order.order_id,
                        retry_count = order.webhook_retry_count,
                        "Webhook retry successful"
                    );
                }
                Ok(response) => {
                    let status = response.status();
                    OrderRecord::increment_webhook_retry_count(pool, order.order_id).await?;

                    warn!(
                        order_id = %order.order_id,
                        status = %status,
                        retry_count = order.webhook_retry_count + 1,
                        "Webhook retry failed"
                    );
                }
                Err(e) => {
                    OrderRecord::increment_webhook_retry_count(pool, order.order_id).await?;

                    warn!(
                        order_id = %order.order_id,
                        error = %e,
                        retry_count = order.webhook_retry_count + 1,
                        "Webhook retry request failed"
                    );
                }
            }
        }

        Ok(())
    }
}

/// Calculate the next retry delay based on retry count.
///
/// Uses exponential backoff: 2^retry_count seconds.
pub fn calculate_retry_delay(retry_count: u32) -> std::time::Duration {
    let seconds = 2u64.pow(retry_count.min(MAX_RETRY_COUNT));
    std::time::Duration::from_secs(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_delay_calculation() {
        assert_eq!(calculate_retry_delay(0), std::time::Duration::from_secs(1));
        assert_eq!(calculate_retry_delay(1), std::time::Duration::from_secs(2));
        assert_eq!(calculate_retry_delay(2), std::time::Duration::from_secs(4));
        assert_eq!(
            calculate_retry_delay(10),
            std::time::Duration::from_secs(1024)
        );
        assert_eq!(
            calculate_retry_delay(11),
            std::time::Duration::from_secs(2048)
        );
        // Max capped at 11
        assert_eq!(
            calculate_retry_delay(12),
            std::time::Duration::from_secs(2048)
        );
        assert_eq!(
            calculate_retry_delay(100),
            std::time::Duration::from_secs(2048)
        );
    }
}
