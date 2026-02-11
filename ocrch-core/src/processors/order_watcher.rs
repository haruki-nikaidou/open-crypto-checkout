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
    Erc20PendingDeposit, Erc20PendingDepositMatch, EtherScanChain, GetErc20DepositsForMatching,
};
use crate::entities::erc20_transfer::{
    Erc20TokenTransfer, Erc20UnmatchedTransfer, GetErc20TokenTransfersUnmatched,
    GetOldUnmatchedErc20TransferIds, MarkErc20TransfersNoMatchedDeposit,
};
use crate::entities::order_records::{OrderRecord, OrderStatus};
use crate::entities::trc20_pending_deposit::{
    GetTrc20DepositsForMatching, Trc20PendingDeposit, Trc20PendingDepositMatch,
};
use crate::entities::trc20_transfer::{
    GetOldUnmatchedTrc20TransferIds, GetTrc20TokenTransfersUnmatched,
    MarkTrc20TransfersNoMatchedDeposit, Trc20TokenTransfer, Trc20UnmatchedTransfer,
};
use crate::events::{
    BlockchainTarget, MatchTick, MatchTickReceiver, WebhookEvent, WebhookEventSender,
};
use crate::framework::DatabaseProcessor;
use compact_str::CompactString;
use itertools::{EitherOrBoth, Itertools};
use kanau::processor::Processor;
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
    async fn match_erc20_transfers(
        &self,
        chain: EtherScanChain,
        token: StablecoinName,
    ) -> Result<(), MatchError> {
        let processor = DatabaseProcessor {
            pool: self.pool.clone(),
        };

        // Get pending deposits for this chain-token pair
        let deposits: Vec<PendingDepositMatch> = processor
            .process(GetErc20DepositsForMatching { chain, token })
            .await?
            .into_iter()
            .map(PendingDepositMatch::from)
            .collect();

        if deposits.is_empty() {
            debug!(
                chain = ?chain,
                token = ?token,
                "No pending ERC-20 deposits to match"
            );
            return Ok(());
        }

        // Get unmatched transfers for this chain-token pair
        let transfers: Vec<UnmatchedTransfer> = processor
            .process(GetErc20TokenTransfersUnmatched { chain, token })
            .await?
            .into_iter()
            .map(UnmatchedTransfer::from)
            .collect();

        if transfers.is_empty() {
            debug!(
                chain = ?chain,
                token = ?token,
                "No unmatched ERC-20 transfers"
            );
            return Ok(());
        }

        info!(
            chain = ?chain,
            token = ?token,
            deposits = deposits.len(),
            transfers = transfers.len(),
            "Attempting to match ERC-20 transfers"
        );
        let (matches, _, _) = Self::compute_matches(transfers, deposits);

        if !matches.is_empty() {
            let (transfer_ids, deposit_ids, order_ids): (Vec<_>, Vec<_>, Vec<_>) = matches
                .into_iter()
                .inspect(|m| {
                    info!(
                        chain = ?chain,
                        token = ?token,
                        transfer_id = m.transfer_id,
                        deposit_id = m.deposit_id,
                        order_id = %m.order_id,
                        "Matched ERC-20 transfer to deposit"
                    );
                })
                .map(|m| (m.transfer_id, m.deposit_id, m.order_id))
                .multiunzip();

            // Execute all matches in a single transaction — O(1) DB operations
            // Exactly 4 SQL statements regardless of the number of matches.
            let mut tx = self.pool.begin().await?;

            Erc20TokenTransfer::mark_matched_many_tx(&mut tx, &transfer_ids, &deposit_ids).await?;
            OrderRecord::update_status_many_tx(&mut tx, &order_ids, OrderStatus::Paid).await?;
            Erc20PendingDeposit::delete_for_orders_except_many_tx(
                &mut tx,
                &order_ids,
                &deposit_ids,
            )
            .await?;
            Trc20PendingDeposit::delete_for_orders_many_tx(&mut tx, &order_ids).await?;

            tx.commit().await?;

            for order_id in order_ids {
                let event = WebhookEvent::OrderStatusChanged {
                    order_id,
                    new_status: OrderStatus::Paid,
                };
                if let Err(e) = self.webhook_tx.send(event).await {
                    error!(
                        order_id = %order_id,
                        error = %e,
                        "Failed to send WebhookEvent"
                    );
                }
            }
        }

        // Check for unknown transfers (transfers older than 1 hour with no matched deposit)
        let old_transfers = processor
            .process(GetOldUnmatchedErc20TransferIds { chain, token })
            .await?;

        if !old_transfers.is_empty() {
            processor
                .process(MarkErc20TransfersNoMatchedDeposit {
                    transfer_ids: old_transfers.clone(),
                })
                .await?;

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
        }

        Ok(())
    }

    /// Match TRC-20 transfers to pending deposits.
    async fn match_trc20_transfers(&self, token: StablecoinName) -> Result<(), MatchError> {
        let processor = DatabaseProcessor {
            pool: self.pool.clone(),
        };

        // Get pending deposits for this token
        let deposits: Vec<PendingDepositMatch> = processor
            .process(GetTrc20DepositsForMatching { token })
            .await?
            .into_iter()
            .map(PendingDepositMatch::from)
            .collect();

        if deposits.is_empty() {
            debug!(token = ?token, "No pending TRC-20 deposits to match");
            return Ok(());
        }

        // Get unmatched transfers for this token
        let transfers: Vec<UnmatchedTransfer> = processor
            .process(GetTrc20TokenTransfersUnmatched { token })
            .await?
            .into_iter()
            .map(UnmatchedTransfer::from)
            .collect();

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
        let (matches, _, _) = Self::compute_matches(transfers, deposits);

        if !matches.is_empty() {
            let (transfer_ids, deposit_ids, order_ids): (Vec<_>, Vec<_>, Vec<_>) = matches
                .into_iter()
                .inspect(|m| {
                    info!(
                        token = ?token,
                        transfer_id = m.transfer_id,
                        deposit_id = m.deposit_id,
                        order_id = %m.order_id,
                        "Matched TRC-20 transfer to deposit"
                    );
                })
                .map(|m| (m.transfer_id, m.deposit_id, m.order_id))
                .multiunzip();

            // Execute all matches in a single transaction — O(1) DB operations
            // Exactly 4 SQL statements regardless of the number of matches.
            let mut tx = self.pool.begin().await?;

            Trc20TokenTransfer::mark_matched_many_tx(&mut tx, &transfer_ids, &deposit_ids).await?;
            OrderRecord::update_status_many_tx(&mut tx, &order_ids, OrderStatus::Paid).await?;
            Trc20PendingDeposit::delete_for_orders_except_many_tx(
                &mut tx,
                &order_ids,
                &deposit_ids,
            )
            .await?;
            Erc20PendingDeposit::delete_for_orders_many_tx(&mut tx, &order_ids).await?;

            tx.commit().await?;

            for order_id in order_ids {
                let event = WebhookEvent::OrderStatusChanged {
                    order_id,
                    new_status: OrderStatus::Paid,
                };
                if let Err(e) = self.webhook_tx.send(event).await {
                    error!(
                        order_id = %order_id,
                        error = %e,
                        "Failed to send WebhookEvent"
                    );
                }
            }
        }

        // Check for unknown transfers (transfers older than 1 hour with no matched deposit)
        let old_transfers = processor
            .process(GetOldUnmatchedTrc20TransferIds { token })
            .await?;

        if !old_transfers.is_empty() {
            processor
                .process(MarkTrc20TransfersNoMatchedDeposit {
                    transfer_ids: old_transfers.clone(),
                })
                .await?;

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
        }

        Ok(())
    }

    /// Compute all matches between transfers and deposits in memory.
    ///
    /// For the same wallet address, deposit amounts should be guaranteed to be unique.
    ///
    /// O(m + n) where m = transfers.len() and n = deposits.len().
    fn compute_matches(
        transfers: Vec<UnmatchedTransfer>,
        deposits: Vec<PendingDepositMatch>,
    ) -> (
        Vec<MatchResult>,
        Vec<UnmatchedTransfer>,
        Vec<PendingDepositMatch>,
    ) {
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
        // Tuple struct for sorting by value then address
        struct DepositKey(Decimal, CompactString);
        let mut transfers = transfers
            .into_iter()
            .map(|t| {
                let to_address = t.to_address.to_lowercase();
                (DepositKey(t.value, to_address.into()), t)
            })
            .collect::<Vec<_>>();
        let mut deposits = deposits
            .into_iter()
            .map(|d| {
                let wallet_address = d.wallet_address.to_lowercase();
                (DepositKey(d.value, wallet_address.into()), d)
            })
            .collect::<Vec<_>>();
        transfers.sort_by_key(|(key, _)| key.to_owned());
        deposits.sort_by_key(|(key, _)| key.to_owned());

        let sorted_transfers = transfers;
        let sorted_deposits = deposits;

        let results: Vec<_> = sorted_transfers
            .into_iter()
            .merge_join_by(sorted_deposits.into_iter(), |(k1, _), (k2, _)| k1.cmp(k2))
            .map(|eob| match eob {
                EitherOrBoth::Left((_, t)) => EitherOrBoth::Left(t),
                EitherOrBoth::Right((_, d)) => EitherOrBoth::Right(d),
                EitherOrBoth::Both((_, t), (_, d)) => EitherOrBoth::Both(t, d),
            })
            .collect();

        let mut matched: Vec<MatchResult> = Vec::new();
        let mut left_only: Vec<UnmatchedTransfer> = Vec::new();
        let mut right_only: Vec<PendingDepositMatch> = Vec::new();

        for result in results {
            match result {
                EitherOrBoth::Left(t) => left_only.push(t),
                EitherOrBoth::Right(d) => right_only.push(d),
                EitherOrBoth::Both(t, d) => matched.push(MatchResult {
                    transfer_id: t.id,
                    deposit_id: d.id,
                    order_id: d.order_id,
                }),
            }
        }
        (matched, left_only, right_only)
    }
}
