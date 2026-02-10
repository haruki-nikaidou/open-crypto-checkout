use crate::entities::erc20_pending_deposit::EtherScanChain;
use crate::entities::{StablecoinName, TransferStatus};
use crate::framework::DatabaseProcessor;
use kanau::processor::Processor;
use rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Erc20TokenTransfer {
    pub id: i64,
    pub token_name: StablecoinName,
    pub chain: EtherScanChain,
    pub from_address: String,
    pub to_address: String,
    pub txn_hash: String,
    pub value: rust_decimal::Decimal,
    pub block_number: i64,
    pub block_timestamp: i64,
    pub blockchain_confirmed: bool,
    pub created_at: time::PrimitiveDateTime,
    pub status: TransferStatus,
    pub fulfillment_id: Option<i64>,
}

/// Data for inserting a new ERC-20 transfer.
#[derive(Debug, Clone)]
pub struct Erc20TransferInsert {
    pub token_name: StablecoinName,
    pub chain: EtherScanChain,
    pub from_address: String,
    pub to_address: String,
    pub txn_hash: String,
    pub value: Decimal,
    pub block_number: i64,
    pub block_timestamp: i64,
}

/// An unmatched transfer for matching operations.
#[derive(Debug, Clone)]
pub struct Erc20UnmatchedTransfer {
    pub id: i64,
    pub to_address: String,
    pub value: Decimal,
    pub block_timestamp: i64,
}

/// Sync cursor from the erc20_sync_cursor materialized view.
/// Contains the block number to start syncing from.
#[derive(Debug, Clone)]
pub struct Erc20SyncCursor {
    pub chain: EtherScanChain,
    pub token_name: StablecoinName,
    /// The block number to start syncing from.
    /// This is either:
    /// - The earliest block of unconfirmed transfers within the last 1 day, or
    /// - The latest block number if all recent transfers are confirmed.
    pub cursor_block_number: i64,
    /// Whether there are unconfirmed transfers within the last 1 day.
    pub has_pending_confirmation: bool,
}

#[derive(Debug, Clone)]
/// Get the sync cursor from the materialized view for a chain-token pair.
///
/// The cursor implements the algorithm:
/// 1. If there are unconfirmed transfers within the last 1 day, return the earliest block number
/// 2. Otherwise, return the latest block number
/// 3. If no transfers exist, return None
pub struct GetErc20TokenTransSyncCursor {
    pub chain: EtherScanChain,
    pub token: StablecoinName,
}

impl Processor<GetErc20TokenTransSyncCursor> for DatabaseProcessor {
    type Output = Option<Erc20SyncCursor>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:GetErc20TokenTransSyncCursor")]
    async fn process(
        &self,
        query: GetErc20TokenTransSyncCursor,
    ) -> Result<Option<Erc20SyncCursor>, sqlx::Error> {
        let cursor = sqlx::query_as!(
            Erc20SyncCursor,
            r#"
            SELECT
                chain as "chain!: EtherScanChain",
                token_name as "token_name!: StablecoinName",
                cursor_block_number as "cursor_block_number!",
                has_pending_confirmation as "has_pending_confirmation!"
            FROM erc20_sync_cursor
            WHERE chain = $1 AND token_name = $2
            "#,
            query.chain as EtherScanChain,
            query.token as StablecoinName,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(cursor)
    }
}

#[derive(Debug, Clone)]
/// Insert multiple transfers in a single query.
///
/// Uses QueryBuilder for efficient bulk insert with ON CONFLICT DO NOTHING.
/// Returns the number of rows actually inserted (excluding duplicates).
pub struct InsertManyErc20TokenTransfers {
    pub transfers: Vec<Erc20TransferInsert>,
}

impl Processor<InsertManyErc20TokenTransfers> for DatabaseProcessor {
    type Output = u64;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:InsertManyErc20TokenTransfers")]
    async fn process(&self, insert: InsertManyErc20TokenTransfers) -> Result<u64, sqlx::Error> {
        if insert.transfers.is_empty() {
            return Ok(0);
        }

        let mut query_builder = sqlx::QueryBuilder::new(
            "INSERT INTO erc20_token_transfers \
            (token_name, chain, from_address, to_address, txn_hash, value, block_number, block_timestamp) ",
        );

        query_builder.push_values(insert.transfers, |mut b, transfer| {
            b.push_bind(transfer.token_name)
                .push_bind(transfer.chain)
                .push_bind(transfer.from_address)
                .push_bind(transfer.to_address)
                .push_bind(transfer.txn_hash)
                .push_bind(transfer.value)
                .push_bind(transfer.block_number)
                .push_bind(transfer.block_timestamp);
        });

        query_builder.push(" ON CONFLICT (txn_hash, chain) DO NOTHING");

        let result = query_builder.build().execute(&self.pool).await?;
        Ok(result.rows_affected())
    }
}

#[derive(Debug, Clone)]
/// Get unmatched transfers that are waiting for a deposit match.
pub struct GetErc20TokenTransfersUnmatched {
    pub chain: EtherScanChain,
    pub token: StablecoinName,
}

impl Processor<GetErc20TokenTransfersUnmatched> for DatabaseProcessor {
    type Output = Vec<Erc20UnmatchedTransfer>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:GetErc20TokenTransfersUnmatched")]
    async fn process(
        &self,
        query: GetErc20TokenTransfersUnmatched,
    ) -> Result<Vec<Erc20UnmatchedTransfer>, sqlx::Error> {
        let GetErc20TokenTransfersUnmatched { chain, token } = query;
        let transfers = sqlx::query_as!(
            Erc20UnmatchedTransfer,
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
}

#[derive(Debug, Clone)]
/// Get IDs of old unmatched transfers (older than 1 hour) for marking as unknown.
pub struct GetOldUnmatchedErc20TransferIds {
    pub chain: EtherScanChain,
    pub token: StablecoinName,
}

impl Processor<GetOldUnmatchedErc20TransferIds> for DatabaseProcessor {
    type Output = Vec<i64>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:GetOldUnmatchedErc20TransferIds")]
    async fn process(
        &self,
        query: GetOldUnmatchedErc20TransferIds,
    ) -> Result<Vec<i64>, sqlx::Error> {
        let ids = sqlx::query_scalar!(
            r#"
            SELECT id
            FROM erc20_token_transfers
            WHERE chain = $1 
              AND token_name = $2
              AND status = 'waiting_for_match'
              AND blockchain_confirmed = true
              AND created_at < NOW() - INTERVAL '1 hour'
            "#,
            query.chain as EtherScanChain,
            query.token as StablecoinName,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(ids)
    }
}

#[derive(Debug, Clone)]
/// Mark multiple transfers as having no matched deposit in a single query.
///
/// Returns the number of rows updated.
pub struct MarkErc20TransfersNoMatchedDeposit {
    pub transfer_ids: Vec<i64>,
}

impl Processor<MarkErc20TransfersNoMatchedDeposit> for DatabaseProcessor {
    type Output = u64;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:MarkErc20TransfersNoMatchedDeposit")]
    async fn process(&self, cmd: MarkErc20TransfersNoMatchedDeposit) -> Result<u64, sqlx::Error> {
        if cmd.transfer_ids.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query!(
            r#"
            UPDATE erc20_token_transfers
            SET status = 'no_matched_deposit'
            WHERE id = ANY($1)
            "#,
            &cmd.transfer_ids,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

impl Erc20TokenTransfer {
    /// Mark multiple transfers as matched with their fulfillment IDs in a single query.
    pub async fn mark_matched_many_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        transfer_ids: &[i64],
        fulfillment_ids: &[i64],
    ) -> Result<u64, sqlx::Error> {
        if transfer_ids.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query(
            r#"
            UPDATE erc20_token_transfers AS t
            SET status = 'matched', fulfillment_id = u.fulfillment_id
            FROM UNNEST($1::bigint[], $2::bigint[]) AS u(id, fulfillment_id)
            WHERE t.id = u.id
            "#,
        )
        .bind(transfer_ids)
        .bind(fulfillment_ids)
        .execute(&mut **tx)
        .await?;
        Ok(result.rows_affected())
    }
}
