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
    Erc20PendingDepositMatch, EtherScanChain, GetErc20DepositsForMatching,
};
use crate::entities::erc20_transfer::{
    Erc20UnmatchedTransfer, GetErc20TokenTransfersUnmatched, GetOldUnmatchedErc20TransferIds,
    HandleErc20MatchedTrans, MarkErc20TransfersNoMatchedDeposit,
};
use crate::entities::order_records::OrderStatus;
use crate::entities::trc20_pending_deposit::{
    GetTrc20DepositsForMatching, Trc20PendingDepositMatch,
};
use crate::entities::trc20_transfer::{
    GetOldUnmatchedTrc20TransferIds, GetTrc20TokenTransfersUnmatched, HandleTrc20MatchedTrans,
    MarkTrc20TransfersNoMatchedDeposit, Trc20UnmatchedTransfer,
};
use crate::events::{
    BlockchainTarget, MatchTick, MatchTickReceiver, WebhookEvent, WebhookEventSender,
};
use crate::framework::DatabaseProcessor;
use compact_str::CompactString;
use itertools::{EitherOrBoth, Itertools};
use kanau::processor::Processor;
use rust_decimal::Decimal;
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
}

impl From<Erc20PendingDepositMatch> for PendingDepositMatch {
    fn from(d: Erc20PendingDepositMatch) -> Self {
        Self {
            id: d.id,
            order_id: d.order_id,
            wallet_address: d.wallet_address,
            value: d.value,
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
        }
    }
}

/// A generic unmatched transfer that needs matching.
#[derive(Debug)]
struct UnmatchedTransfer {
    id: i64,
    to_address: String,
    value: Decimal,
}

impl From<Erc20UnmatchedTransfer> for UnmatchedTransfer {
    fn from(t: Erc20UnmatchedTransfer) -> Self {
        Self {
            id: t.id,
            to_address: t.to_address,
            value: t.value,
        }
    }
}

impl From<Trc20UnmatchedTransfer> for UnmatchedTransfer {
    fn from(t: Trc20UnmatchedTransfer) -> Self {
        Self {
            id: t.id,
            to_address: t.to_address,
            value: t.value,
        }
    }
}

#[derive(Debug)]
struct MatchResult {
    transfer_id: i64,
    deposit_id: i64,
    order_id: Uuid,
}

/// OrderBookWatcher handles matching pending deposits to blockchain transfers.
pub struct OrderBookWatcher {
    pub processor: DatabaseProcessor,
}

impl OrderBookWatcher {
    /// Run the OrderBookWatcher.
    pub async fn run(
        self,
        mut shutdown_rx: watch::Receiver<bool>,
        mut match_rx: MatchTickReceiver,
        webhook_tx: WebhookEventSender,
    ) {
        info!("OrderBookWatcher started");

        loop {
            tokio::select! {
                biased;

                // Check for shutdown
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("OrderBookWatcher received shutdown signal");
                        break;
                    }
                }

                // Receive MatchTick events
                Some(tick) = match_rx.recv() => {
                    debug!(
                        blockchain = %tick.blockchain,
                        token = ?tick.token,
                        transfers_synced = tick.transfers_synced,
                        "Received MatchTick"
                    );

                    match self.process(tick).await {
                        Ok(events) => {
                            for event in events {
                                if let Err(e) = webhook_tx.send(event).await {
                                    warn!(
                                        error = %e,
                                        "Failed to send WebhookEvent"
                                    );
                                }
                            }
                        },
                        Err(e) => {
                            error!(
                                blockchain = %tick.blockchain,
                                token = ?tick.token,
                                error = %e,
                                "Failed to process MatchTick"
                            );
                        }
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
            .merge_join_by(sorted_deposits, |(k1, _), (k2, _)| k1.cmp(k2))
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

#[derive(Debug, Clone)]
/// Match ERC-20 transfers to pending deposits.
pub struct Erc20Matching {
    chain: EtherScanChain,
    token: StablecoinName,
}

impl Processor<Erc20Matching> for OrderBookWatcher {
    type Output = Vec<WebhookEvent>;
    type Error = MatchError;

    async fn process(&self, cmd: Erc20Matching) -> Result<Vec<WebhookEvent>, MatchError> {
        let Erc20Matching { chain, token } = cmd;
        // Get pending deposits for this chain-token pair
        let deposits: Vec<PendingDepositMatch> = self
            .processor
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
            return Ok(Vec::new());
        }

        // Get unmatched transfers for this chain-token pair
        let transfers: Vec<UnmatchedTransfer> = self
            .processor
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
            return Ok(Vec::new());
        }

        info!(
            chain = ?chain,
            token = ?token,
            deposits = deposits.len(),
            transfers = transfers.len(),
            "Attempting to match ERC-20 transfers"
        );
        let (matches, _, _) = Self::compute_matches(transfers, deposits);

        // Check for unknown transfers (transfers older than 1 hour with no matched deposit)
        let old_transfers = self
            .processor
            .process(GetOldUnmatchedErc20TransferIds { chain, token })
            .await?;

        let mut events = Vec::new();

        if !old_transfers.is_empty() {
            self.processor
                .process(MarkErc20TransfersNoMatchedDeposit {
                    transfer_ids: old_transfers.clone(),
                })
                .await?;

            let mapped = old_transfers.into_iter().map(|transfer_id| {
                WebhookEvent::UnknownTransferReceived {
                    transfer_id,
                    blockchain: BlockchainTarget::Erc20(chain),
                }
            });
            events.extend(mapped);
        }

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
            self.processor
                .process(HandleErc20MatchedTrans {
                    transfer_ids,
                    deposit_ids,
                    order_ids: order_ids.clone(),
                })
                .await?;
            let mapped = order_ids
                .into_iter()
                .map(|order_id| WebhookEvent::OrderStatusChanged {
                    order_id,
                    new_status: OrderStatus::Paid,
                });
            events.extend(mapped);
        }
        Ok(events)
    }
}

#[derive(Debug, Clone)]
pub struct Trc20Matching {
    token: StablecoinName,
}

impl Processor<Trc20Matching> for OrderBookWatcher {
    type Output = Vec<WebhookEvent>;
    type Error = MatchError;
    #[tracing::instrument(skip_all, err, name = "OrderBookWatcher:Trc20Matching")]
    async fn process(&self, cmd: Trc20Matching) -> Result<Vec<WebhookEvent>, MatchError> {
        let Trc20Matching { token } = cmd;

        // Get pending deposits for this token
        let deposits: Vec<PendingDepositMatch> = self
            .processor
            .process(GetTrc20DepositsForMatching { token })
            .await?
            .into_iter()
            .map(PendingDepositMatch::from)
            .collect();

        if deposits.is_empty() {
            debug!(token = ?token, "No pending TRC-20 deposits to match");
            return Ok(Vec::new());
        }

        // Get unmatched transfers for this token
        let transfers: Vec<UnmatchedTransfer> = self
            .processor
            .process(GetTrc20TokenTransfersUnmatched { token })
            .await?
            .into_iter()
            .map(UnmatchedTransfer::from)
            .collect();

        if transfers.is_empty() {
            debug!(token = ?token, "No unmatched TRC-20 transfers");
            return Ok(Vec::new());
        }

        debug!(
            token = ?token,
            deposits = deposits.len(),
            transfers = transfers.len(),
            "Attempting to match TRC-20 transfers"
        );
        let (matches, _, _) = Self::compute_matches(transfers, deposits);

        // Check for unknown transfers (transfers older than 1 hour with no matched deposit)
        let old_transfers = self
            .processor
            .process(GetOldUnmatchedTrc20TransferIds { token })
            .await?;

        let mut events = Vec::new();
        if !old_transfers.is_empty() {
            self.processor
                .process(MarkTrc20TransfersNoMatchedDeposit {
                    transfer_ids: old_transfers.clone(),
                })
                .await?;
            let mapped = old_transfers.into_iter().map(|transfer_id| {
                WebhookEvent::UnknownTransferReceived {
                    transfer_id,
                    blockchain: BlockchainTarget::Trc20,
                }
            });
            events.extend(mapped);
        }

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
            self.processor
                .process(HandleTrc20MatchedTrans {
                    transfer_ids,
                    deposit_ids,
                    order_ids: order_ids.clone(),
                })
                .await?;

            let mapped = order_ids
                .into_iter()
                .map(|order_id| WebhookEvent::OrderStatusChanged {
                    order_id,
                    new_status: OrderStatus::Paid,
                });
            events.extend(mapped);
        }
        Ok(events)
    }
}

impl Processor<MatchTick> for OrderBookWatcher {
    type Output = Vec<WebhookEvent>;
    type Error = MatchError;
    #[tracing::instrument(skip_all, err, name = "OrderBookWatcher:MatchTick")]
    async fn process(&self, tick: MatchTick) -> Result<Vec<WebhookEvent>, MatchError> {
        match tick.blockchain {
            BlockchainTarget::Erc20(chain) => {
                self.process(Erc20Matching {
                    chain,
                    token: tick.token,
                })
                .await
            }
            BlockchainTarget::Trc20 => self.process(Trc20Matching { token: tick.token }).await,
        }
    }
}
