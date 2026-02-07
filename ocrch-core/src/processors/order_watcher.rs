//! OrderBookWatcher processor.
//!
//! The OrderBookWatcher is responsible for:
//! - Receiving `MatchTick` events
//! - Querying pending deposits for the given blockchain-token pair
//! - Querying unmatched transfers in the time window
//! - Matching transfers to deposits by wallet address and amount
//! - Updating transfer status to `Matched` and linking `fulfillment_id`
//! - Emitting `WebhookEvent::OrderStatusChanged` for successful matches

use crate::entities::StablecoinName;
use crate::entities::erc20_pending_deposit::{
    Erc20PendingDeposit, Erc20PendingDepositMatch, EtherScanChain,
};
use crate::entities::erc20_transfer::{Erc20TokenTransfer, Erc20UnmatchedTransfer};
use crate::entities::order_records::{OrderRecord, OrderStatus};
use crate::entities::trc20_pending_deposit::{Trc20PendingDeposit, Trc20PendingDepositMatch};
use crate::entities::trc20_transfer::{Trc20TokenTransfer, Trc20UnmatchedTransfer};
use crate::events::{
    BlockchainTarget, MatchTick, MatchTickReceiver, WebhookEvent, WebhookEventSender,
};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::collections::HashSet;
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

/// A generic pending deposit that can be matched.
#[derive(Debug)]
struct PendingDepositMatch {
    id: i64,
    order_id: Uuid,
    wallet_address: String,
    value: Decimal,
    started_at_timestamp: i64,
}

impl From<Erc20PendingDepositMatch> for PendingDepositMatch {
    fn from(d: Erc20PendingDepositMatch) -> Self {
        Self {
            id: d.id,
            order_id: d.order_id,
            wallet_address: d.wallet_address,
            value: d.value,
            started_at_timestamp: d.started_at_timestamp,
        }
    }
}

impl From<Trc20PendingDepositMatch> for PendingDepositMatch {
    fn from(d: Trc20PendingDepositMatch) -> Self {
        Self {
            id: d.id,
            order_id: d.order_id,
            wallet_address: d.wallet_address,
            value: d.value,
            started_at_timestamp: d.started_at_timestamp,
        }
    }
}

/// A generic unmatched transfer that needs matching.
#[derive(Debug)]
struct UnmatchedTransfer {
    id: i64,
    to_address: String,
    value: Decimal,
    block_timestamp: i64,
}

impl From<Erc20UnmatchedTransfer> for UnmatchedTransfer {
    fn from(t: Erc20UnmatchedTransfer) -> Self {
        Self {
            id: t.id,
            to_address: t.to_address,
            value: t.value,
            block_timestamp: t.block_timestamp,
        }
    }
}

impl From<Trc20UnmatchedTransfer> for UnmatchedTransfer {
    fn from(t: Trc20UnmatchedTransfer) -> Self {
        Self {
            id: t.id,
            to_address: t.to_address,
            value: t.value,
            block_timestamp: t.block_timestamp,
        }
    }
}

/// A successful match between a transfer and a deposit, computed in memory.
#[derive(Debug)]
struct MatchResult {
    transfer_id: i64,
    deposit_id: i64,
    order_id: Uuid,
}

/// OrderBookWatcher handles matching pending deposits to blockchain transfers.
pub struct OrderBookWatcher {
    pub pool: PgPool,
    pub match_rx: MatchTickReceiver,
    pub webhook_tx: WebhookEventSender,
    pub shutdown_rx: watch::Receiver<bool>,
}

impl OrderBookWatcher {
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
            BlockchainTarget::Erc20(chain) => self.match_erc20_transfers(chain, tick.token).await,
            BlockchainTarget::Trc20 => self.match_trc20_transfers(tick.token).await,
        }
    }

    /// Match ERC-20 transfers to pending deposits.
    ///
    /// All matches are computed in memory (O(m*n)) then committed in a single
    /// database transaction (O(1) DB operations).
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

        // Compute all matches in memory — O(m*n)
        let matches = self.compute_matches(&transfers, &deposits);

        if !matches.is_empty() {
            // Log each match
            for m in &matches {
                info!(
                    chain = ?chain,
                    token = ?token,
                    transfer_id = m.transfer_id,
                    deposit_id = m.deposit_id,
                    order_id = %m.order_id,
                    "Matched ERC-20 transfer to deposit"
                );
            }

            // Prepare batch arrays from computed matches
            let transfer_ids: Vec<i64> = matches.iter().map(|m| m.transfer_id).collect();
            let deposit_ids: Vec<i64> = matches.iter().map(|m| m.deposit_id).collect();
            let order_ids: Vec<Uuid> = matches.iter().map(|m| m.order_id).collect();

            // Execute all matches in a single transaction — O(1) DB operations
            // Exactly 4 SQL statements regardless of the number of matches.
            let mut tx = self.pool.begin().await?;

            // 1. Batch mark transfers as matched
            Erc20TokenTransfer::mark_matched_many_tx(&mut tx, &transfer_ids, &deposit_ids).await?;

            // 2. Batch update order statuses to Paid
            OrderRecord::update_status_many_tx(&mut tx, &order_ids, OrderStatus::Paid).await?;

            // 3. Batch delete ERC-20 pending deposits (keep matched ones)
            Erc20PendingDeposit::delete_for_orders_except_many_tx(
                &mut tx,
                &order_ids,
                &deposit_ids,
            )
            .await?;

            // 4. Batch delete TRC-20 pending deposits for matched orders
            Trc20PendingDeposit::delete_for_orders_many_tx(&mut tx, &order_ids).await?;

            tx.commit().await?;

            // Emit webhook events after successful commit
            for m in &matches {
                let event = WebhookEvent::OrderStatusChanged {
                    order_id: m.order_id,
                    new_status: OrderStatus::Paid,
                };

                if let Err(e) = self.webhook_tx.send(event).await {
                    error!(
                        order_id = %m.order_id,
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
    ///
    /// All matches are computed in memory (O(m*n)) then committed in a single
    /// database transaction (O(1) DB operations).
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

        // Compute all matches in memory — O(m*n)
        let matches = self.compute_matches(&transfers, &deposits);

        if !matches.is_empty() {
            // Log each match
            for m in &matches {
                info!(
                    token = ?token,
                    transfer_id = m.transfer_id,
                    deposit_id = m.deposit_id,
                    order_id = %m.order_id,
                    "Matched TRC-20 transfer to deposit"
                );
            }

            // Prepare batch arrays from computed matches
            let transfer_ids: Vec<i64> = matches.iter().map(|m| m.transfer_id).collect();
            let deposit_ids: Vec<i64> = matches.iter().map(|m| m.deposit_id).collect();
            let order_ids: Vec<Uuid> = matches.iter().map(|m| m.order_id).collect();

            // Execute all matches in a single transaction — O(1) DB operations
            // Exactly 4 SQL statements regardless of the number of matches.
            let mut tx = self.pool.begin().await?;

            // 1. Batch mark transfers as matched
            Trc20TokenTransfer::mark_matched_many_tx(&mut tx, &transfer_ids, &deposit_ids).await?;

            // 2. Batch update order statuses to Paid
            OrderRecord::update_status_many_tx(&mut tx, &order_ids, OrderStatus::Paid).await?;

            // 3. Batch delete TRC-20 pending deposits (keep matched ones)
            Trc20PendingDeposit::delete_for_orders_except_many_tx(
                &mut tx,
                &order_ids,
                &deposit_ids,
            )
            .await?;

            // 4. Batch delete ERC-20 pending deposits for matched orders
            Erc20PendingDeposit::delete_for_orders_many_tx(&mut tx, &order_ids).await?;

            tx.commit().await?;

            // Emit webhook events after successful commit
            for m in &matches {
                let event = WebhookEvent::OrderStatusChanged {
                    order_id: m.order_id,
                    new_status: OrderStatus::Paid,
                };

                if let Err(e) = self.webhook_tx.send(event).await {
                    error!(
                        order_id = %m.order_id,
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

    /// Compute all matches between transfers and deposits in memory.
    ///
    /// O(m*n) where m = transfers.len() and n = deposits.len().
    /// Each deposit is matched at most once (first-come-first-served by
    /// transfer ordering). Each transfer is matched to at most one deposit.
    fn compute_matches(
        &self,
        transfers: &[UnmatchedTransfer],
        deposits: &[PendingDepositMatch],
    ) -> Vec<MatchResult> {
        let mut matched_deposit_ids: HashSet<i64> = HashSet::new();
        let mut results = Vec::new();

        for transfer in transfers {
            for deposit in deposits {
                // Skip deposits that have already been matched
                if matched_deposit_ids.contains(&deposit.id) {
                    continue;
                }

                // Wallet address must match (case-insensitive)
                let address_matches =
                    deposit.wallet_address.to_lowercase() == transfer.to_address.to_lowercase();

                // Value must match exactly
                let value_matches = deposit.value == transfer.value;

                // Transfer must be after deposit was created
                let time_valid = transfer.block_timestamp >= deposit.started_at_timestamp;

                if address_matches && value_matches && time_valid {
                    matched_deposit_ids.insert(deposit.id);
                    results.push(MatchResult {
                        transfer_id: transfer.id,
                        deposit_id: deposit.id,
                        order_id: deposit.order_id,
                    });
                    break; // Move to next transfer
                }
            }
        }

        results
    }

    /// Get pending ERC-20 deposits for matching.
    async fn get_erc20_pending_deposits(
        &self,
        chain: EtherScanChain,
        token: StablecoinName,
    ) -> Result<Vec<PendingDepositMatch>, MatchError> {
        let deposits = Erc20PendingDeposit::get_for_matching(&self.pool, chain, token)
            .await?
            .into_iter()
            .map(PendingDepositMatch::from)
            .collect();
        Ok(deposits)
    }

    /// Get pending TRC-20 deposits for matching.
    async fn get_trc20_pending_deposits(
        &self,
        token: StablecoinName,
    ) -> Result<Vec<PendingDepositMatch>, MatchError> {
        let deposits = Trc20PendingDeposit::get_for_matching(&self.pool, token)
            .await?
            .into_iter()
            .map(PendingDepositMatch::from)
            .collect();
        Ok(deposits)
    }

    /// Get unmatched ERC-20 transfers.
    async fn get_unmatched_erc20_transfers(
        &self,
        chain: EtherScanChain,
        token: StablecoinName,
    ) -> Result<Vec<UnmatchedTransfer>, MatchError> {
        let transfers = Erc20TokenTransfer::get_unmatched(&self.pool, chain, token)
            .await?
            .into_iter()
            .map(UnmatchedTransfer::from)
            .collect();
        Ok(transfers)
    }

    /// Get unmatched TRC-20 transfers.
    async fn get_unmatched_trc20_transfers(
        &self,
        token: StablecoinName,
    ) -> Result<Vec<UnmatchedTransfer>, MatchError> {
        let transfers = Trc20TokenTransfer::get_unmatched(&self.pool, token)
            .await?
            .into_iter()
            .map(UnmatchedTransfer::from)
            .collect();
        Ok(transfers)
    }

    /// Check for unknown ERC-20 transfers and emit webhook events.
    async fn check_erc20_unknown_transfers(
        &self,
        chain: EtherScanChain,
        token: StablecoinName,
    ) -> Result<(), MatchError> {
        // Find transfers that have been waiting too long and mark as no_matched_deposit
        // These are transfers older than 1 hour that haven't been matched
        let old_transfers =
            Erc20TokenTransfer::get_old_unmatched_ids(&self.pool, chain, token).await?;

        if old_transfers.is_empty() {
            return Ok(());
        }

        // Batch update all transfers at once
        Erc20TokenTransfer::mark_no_matched_deposit_many(&self.pool, &old_transfers).await?;

        // Emit webhook events for each unknown transfer
        for transfer_id in old_transfers {
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
        let old_transfers = Trc20TokenTransfer::get_old_unmatched_ids(&self.pool, token).await?;

        if old_transfers.is_empty() {
            return Ok(());
        }

        // Batch update all transfers at once
        Trc20TokenTransfer::mark_no_matched_deposit_many(&self.pool, &old_transfers).await?;

        // Emit webhook events for each unknown transfer
        for transfer_id in old_transfers {
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
