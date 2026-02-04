//! OrderBookWatcher processor.
//!
//! The OrderBookWatcher is responsible for:
//! - Receiving `MatchTick` events
//! - Querying pending deposits for the given blockchain-token pair
//! - Querying unmatched transfers in the time window
//! - Matching transfers to deposits by wallet address and amount
//! - Updating transfer status to `Matched` and linking `fulfillment_id`
//! - Emitting `WebhookEvent::OrderStatusChanged` for successful matches

use crate::entities::erc20_pending_deposit::EtherScanChain;
use crate::entities::order_records::OrderStatus;
use crate::entities::StablecoinName;
use crate::events::{
    BlockchainTarget, MatchTick, MatchTickReceiver, WebhookEvent, WebhookEventSender,
};
use rust_decimal::Decimal;
use sqlx::PgPool;
use thiserror::Error;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Errors that can occur during order matching.
#[derive(Debug, Error)]
pub enum MatchError {
    /// Database error
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// A pending deposit that can be matched.
#[derive(Debug)]
struct PendingDepositMatch {
    id: i64,
    order_id: Uuid,
    wallet_address: String,
    value: Decimal,
    started_at_timestamp: i64,
}

/// An unmatched transfer that needs matching.
#[derive(Debug)]
struct UnmatchedTransfer {
    id: i64,
    to_address: String,
    value: Decimal,
    block_timestamp: i64,
}

/// OrderBookWatcher handles matching pending deposits to blockchain transfers.
pub struct OrderBookWatcher {
    pool: PgPool,
    match_rx: MatchTickReceiver,
    webhook_tx: WebhookEventSender,
    shutdown_rx: watch::Receiver<bool>,
}

impl OrderBookWatcher {
    /// Create a new OrderBookWatcher.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `match_rx` - Receiver for MatchTick events
    /// * `webhook_tx` - Sender for WebhookEvent events
    /// * `shutdown_rx` - Receiver for shutdown signal
    pub fn new(
        pool: PgPool,
        match_rx: MatchTickReceiver,
        webhook_tx: WebhookEventSender,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        Self {
            pool,
            match_rx,
            webhook_tx,
            shutdown_rx,
        }
    }

    /// Run the OrderBookWatcher.
    pub async fn run(mut self) {
        info!("OrderBookWatcher started");

        loop {
            tokio::select! {
                biased;

                // Check for shutdown
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        info!("OrderBookWatcher received shutdown signal");
                        break;
                    }
                }

                // Receive MatchTick events
                Some(tick) = self.match_rx.recv() => {
                    debug!(
                        blockchain = %tick.blockchain,
                        token = ?tick.token,
                        transfers_synced = tick.transfers_synced,
                        "Received MatchTick"
                    );

                    if let Err(e) = self.process_match_tick(&tick).await {
                        error!(
                            blockchain = %tick.blockchain,
                            token = ?tick.token,
                            error = %e,
                            "Failed to process MatchTick"
                        );
                    }
                }

                else => {
                    info!("MatchTick channel closed");
                    break;
                }
            }
        }

        info!("OrderBookWatcher shutdown complete");
    }

    /// Process a MatchTick event.
    async fn process_match_tick(&self, tick: &MatchTick) -> Result<(), MatchError> {
        match tick.blockchain {
            BlockchainTarget::Erc20(chain) => {
                self.match_erc20_transfers(chain, tick.token).await
            }
            BlockchainTarget::Trc20 => {
                self.match_trc20_transfers(tick.token).await
            }
        }
    }

    /// Match ERC-20 transfers to pending deposits.
    async fn match_erc20_transfers(
        &self,
        chain: EtherScanChain,
        token: StablecoinName,
    ) -> Result<(), MatchError> {
        // Get pending deposits for this chain-token pair
        let deposits = self.get_erc20_pending_deposits(chain, token).await?;

        if deposits.is_empty() {
            debug!(
                chain = ?chain,
                token = ?token,
                "No pending ERC-20 deposits to match"
            );
            return Ok(());
        }

        // Get unmatched transfers for this chain-token pair
        let transfers = self.get_unmatched_erc20_transfers(chain, token).await?;

        if transfers.is_empty() {
            debug!(
                chain = ?chain,
                token = ?token,
                "No unmatched ERC-20 transfers"
            );
            return Ok(());
        }

        debug!(
            chain = ?chain,
            token = ?token,
            deposits = deposits.len(),
            transfers = transfers.len(),
            "Attempting to match ERC-20 transfers"
        );

        // Try to match transfers to deposits
        for transfer in transfers {
            if let Some(deposit) = self.find_matching_deposit(&transfer, &deposits) {
                info!(
                    chain = ?chain,
                    token = ?token,
                    transfer_id = transfer.id,
                    deposit_id = deposit.id,
                    order_id = %deposit.order_id,
                    value = %transfer.value,
                    "Matched ERC-20 transfer to deposit"
                );

                // Update transfer and order in a transaction
                self.execute_erc20_match(transfer.id, deposit.id, deposit.order_id, chain)
                    .await?;

                // Emit webhook event
                let event = WebhookEvent::OrderStatusChanged {
                    order_id: deposit.order_id,
                    new_status: OrderStatus::Paid,
                };

                if let Err(e) = self.webhook_tx.send(event).await {
                    error!(
                        order_id = %deposit.order_id,
                        error = %e,
                        "Failed to send WebhookEvent"
                    );
                }
            }
        }

        // Check for unknown transfers (transfers that don't match any deposit)
        self.check_erc20_unknown_transfers(chain, token).await?;

        Ok(())
    }

    /// Match TRC-20 transfers to pending deposits.
    async fn match_trc20_transfers(&self, token: StablecoinName) -> Result<(), MatchError> {
        // Get pending deposits for this token
        let deposits = self.get_trc20_pending_deposits(token).await?;

        if deposits.is_empty() {
            debug!(token = ?token, "No pending TRC-20 deposits to match");
            return Ok(());
        }

        // Get unmatched transfers for this token
        let transfers = self.get_unmatched_trc20_transfers(token).await?;

        if transfers.is_empty() {
            debug!(token = ?token, "No unmatched TRC-20 transfers");
            return Ok(());
        }

        debug!(
            token = ?token,
            deposits = deposits.len(),
            transfers = transfers.len(),
            "Attempting to match TRC-20 transfers"
        );

        // Try to match transfers to deposits
        for transfer in transfers {
            if let Some(deposit) = self.find_matching_deposit(&transfer, &deposits) {
                info!(
                    token = ?token,
                    transfer_id = transfer.id,
                    deposit_id = deposit.id,
                    order_id = %deposit.order_id,
                    value = %transfer.value,
                    "Matched TRC-20 transfer to deposit"
                );

                // Update transfer and order in a transaction
                self.execute_trc20_match(transfer.id, deposit.id, deposit.order_id)
                    .await?;

                // Emit webhook event
                let event = WebhookEvent::OrderStatusChanged {
                    order_id: deposit.order_id,
                    new_status: OrderStatus::Paid,
                };

                if let Err(e) = self.webhook_tx.send(event).await {
                    error!(
                        order_id = %deposit.order_id,
                        error = %e,
                        "Failed to send WebhookEvent"
                    );
                }
            }
        }

        // Check for unknown transfers
        self.check_trc20_unknown_transfers(token).await?;

        Ok(())
    }

    /// Find a matching deposit for a transfer.
    fn find_matching_deposit<'a>(
        &self,
        transfer: &UnmatchedTransfer,
        deposits: &'a [PendingDepositMatch],
    ) -> Option<&'a PendingDepositMatch> {
        deposits.iter().find(|deposit| {
            // Wallet address must match (case-insensitive)
            let address_matches =
                deposit.wallet_address.to_lowercase() == transfer.to_address.to_lowercase();

            // Value must match exactly
            let value_matches = deposit.value == transfer.value;

            // Transfer must be after deposit was created
            let time_valid = transfer.block_timestamp >= deposit.started_at_timestamp;

            address_matches && value_matches && time_valid
        })
    }

    /// Get pending ERC-20 deposits for matching.
    async fn get_erc20_pending_deposits(
        &self,
        chain: EtherScanChain,
        token: StablecoinName,
    ) -> Result<Vec<PendingDepositMatch>, MatchError> {
        let deposits = sqlx::query_as!(
            PendingDepositMatch,
            r#"
            SELECT 
                d.id,
                d."order" as order_id,
                d.wallet_address,
                d.value,
                EXTRACT(EPOCH FROM d.started_at)::bigint as "started_at_timestamp!"
            FROM erc20_pending_deposits d
            JOIN order_records o ON d."order" = o.order_id
            WHERE d.chain = $1 
              AND d.token_name = $2
              AND o.status = 'pending'
            "#,
            chain as EtherScanChain,
            token as StablecoinName,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(deposits)
    }

    /// Get pending TRC-20 deposits for matching.
    async fn get_trc20_pending_deposits(
        &self,
        token: StablecoinName,
    ) -> Result<Vec<PendingDepositMatch>, MatchError> {
        let deposits = sqlx::query_as!(
            PendingDepositMatch,
            r#"
            SELECT 
                d.id,
                d."order" as order_id,
                d.wallet_address,
                d.value,
                EXTRACT(EPOCH FROM d.started_at)::bigint as "started_at_timestamp!"
            FROM trc20_pending_deposits d
            JOIN order_records o ON d."order" = o.order_id
            WHERE d.token_name = $1
              AND o.status = 'pending'
            "#,
            token as StablecoinName,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(deposits)
    }

    /// Get unmatched ERC-20 transfers.
    async fn get_unmatched_erc20_transfers(
        &self,
        chain: EtherScanChain,
        token: StablecoinName,
    ) -> Result<Vec<UnmatchedTransfer>, MatchError> {
        let transfers = sqlx::query_as!(
            UnmatchedTransfer,
            r#"
            SELECT 
                id,
                to_address,
                value,
                block_timestamp
            FROM erc20_token_transfers
            WHERE chain = $1 
              AND token_name = $2
              AND status = 'waiting_for_match'
              AND blockchain_confirmed = true
            ORDER BY block_timestamp ASC
            "#,
            chain as EtherScanChain,
            token as StablecoinName,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(transfers)
    }

    /// Get unmatched TRC-20 transfers.
    async fn get_unmatched_trc20_transfers(
        &self,
        token: StablecoinName,
    ) -> Result<Vec<UnmatchedTransfer>, MatchError> {
        let transfers = sqlx::query_as!(
            UnmatchedTransfer,
            r#"
            SELECT 
                id,
                to_address,
                value,
                block_timestamp
            FROM trc20_token_transfers
            WHERE token_name = $1
              AND status = 'waiting_for_match'
              AND blockchain_confirmed = true
            ORDER BY block_timestamp ASC
            "#,
            token as StablecoinName,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(transfers)
    }

    /// Execute an ERC-20 match in a database transaction.
    async fn execute_erc20_match(
        &self,
        transfer_id: i64,
        deposit_id: i64,
        order_id: Uuid,
        _chain: EtherScanChain,
    ) -> Result<(), MatchError> {
        let mut tx = self.pool.begin().await?;

        // Update transfer status to matched
        sqlx::query!(
            r#"
            UPDATE erc20_token_transfers
            SET status = 'matched', fulfillment_id = $1
            WHERE id = $2
            "#,
            deposit_id,
            transfer_id,
        )
        .execute(&mut *tx)
        .await?;

        // Update order status to paid
        sqlx::query!(
            r#"
            UPDATE order_records
            SET status = 'paid'
            WHERE order_id = $1
            "#,
            order_id,
        )
        .execute(&mut *tx)
        .await?;

        // Delete other pending deposits for this order (keep only the matched one)
        sqlx::query!(
            r#"
            DELETE FROM erc20_pending_deposits
            WHERE "order" = $1 AND id != $2
            "#,
            order_id,
            deposit_id,
        )
        .execute(&mut *tx)
        .await?;

        // Also delete any TRC-20 pending deposits for this order
        sqlx::query!(
            r#"
            DELETE FROM trc20_pending_deposits
            WHERE "order" = $1
            "#,
            order_id,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }

    /// Execute a TRC-20 match in a database transaction.
    async fn execute_trc20_match(
        &self,
        transfer_id: i64,
        deposit_id: i64,
        order_id: Uuid,
    ) -> Result<(), MatchError> {
        let mut tx = self.pool.begin().await?;

        // Update transfer status to matched
        sqlx::query!(
            r#"
            UPDATE trc20_token_transfers
            SET status = 'matched', fulfillment_id = $1
            WHERE id = $2
            "#,
            deposit_id,
            transfer_id,
        )
        .execute(&mut *tx)
        .await?;

        // Update order status to paid
        sqlx::query!(
            r#"
            UPDATE order_records
            SET status = 'paid'
            WHERE order_id = $1
            "#,
            order_id,
        )
        .execute(&mut *tx)
        .await?;

        // Delete other pending deposits for this order (keep only the matched one)
        sqlx::query!(
            r#"
            DELETE FROM trc20_pending_deposits
            WHERE "order" = $1 AND id != $2
            "#,
            order_id,
            deposit_id,
        )
        .execute(&mut *tx)
        .await?;

        // Also delete any ERC-20 pending deposits for this order
        sqlx::query!(
            r#"
            DELETE FROM erc20_pending_deposits
            WHERE "order" = $1
            "#,
            order_id,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }

    /// Check for unknown ERC-20 transfers and emit webhook events.
    async fn check_erc20_unknown_transfers(
        &self,
        chain: EtherScanChain,
        token: StablecoinName,
    ) -> Result<(), MatchError> {
        // Find transfers that have been waiting too long and mark as no_matched_deposit
        // These are transfers older than 1 hour that haven't been matched
        let old_transfers = sqlx::query_scalar!(
            r#"
            SELECT id
            FROM erc20_token_transfers
            WHERE chain = $1 
              AND token_name = $2
              AND status = 'waiting_for_match'
              AND blockchain_confirmed = true
              AND created_at < NOW() - INTERVAL '1 hour'
            "#,
            chain as EtherScanChain,
            token as StablecoinName,
        )
        .fetch_all(&self.pool)
        .await?;

        for transfer_id in old_transfers {
            // Update status
            sqlx::query!(
                r#"
                UPDATE erc20_token_transfers
                SET status = 'no_matched_deposit'
                WHERE id = $1
                "#,
                transfer_id,
            )
            .execute(&self.pool)
            .await?;

            // Emit webhook event for unknown transfer
            let event = WebhookEvent::UnknownTransferReceived {
                transfer_id,
                blockchain: BlockchainTarget::Erc20(chain),
            };

            if let Err(e) = self.webhook_tx.send(event).await {
                warn!(
                    transfer_id = transfer_id,
                    error = %e,
                    "Failed to send unknown transfer WebhookEvent"
                );
            }
        }

        Ok(())
    }

    /// Check for unknown TRC-20 transfers and emit webhook events.
    async fn check_trc20_unknown_transfers(&self, token: StablecoinName) -> Result<(), MatchError> {
        // Find transfers that have been waiting too long and mark as no_matched_deposit
        let old_transfers = sqlx::query_scalar!(
            r#"
            SELECT id
            FROM trc20_token_transfers
            WHERE token_name = $1
              AND status = 'waiting_for_match'
              AND blockchain_confirmed = true
              AND created_at < NOW() - INTERVAL '1 hour'
            "#,
            token as StablecoinName,
        )
        .fetch_all(&self.pool)
        .await?;

        for transfer_id in old_transfers {
            // Update status
            sqlx::query!(
                r#"
                UPDATE trc20_token_transfers
                SET status = 'no_matched_deposit'
                WHERE id = $1
                "#,
                transfer_id,
            )
            .execute(&self.pool)
            .await?;

            // Emit webhook event for unknown transfer
            let event = WebhookEvent::UnknownTransferReceived {
                transfer_id,
                blockchain: BlockchainTarget::Trc20,
            };

            if let Err(e) = self.webhook_tx.send(event).await {
                warn!(
                    transfer_id = transfer_id,
                    error = %e,
                    "Failed to send unknown transfer WebhookEvent"
                );
            }
        }

        Ok(())
    }
}
