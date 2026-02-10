use crate::entities::{StablecoinName, TransferStatus};
use crate::framework::DatabaseProcessor;
use kanau::processor::Processor;
use rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trc20TokenTransfer {
    pub id: i64,
    pub token_name: StablecoinName,
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

/// Data for inserting a new TRC-20 transfer.
#[derive(Debug, Clone)]
pub struct Trc20TransferInsert {
    pub token_name: StablecoinName,
    pub from_address: String,
    pub to_address: String,
    pub txn_hash: String,
    pub value: Decimal,
    pub block_number: i64,
    pub block_timestamp: i64,
}

/// An unmatched transfer for matching operations.
#[derive(Debug, Clone)]
pub struct Trc20UnmatchedTransfer {
    pub id: i64,
    pub to_address: String,
    pub value: Decimal,
    pub block_timestamp: i64,
}

/// Sync cursor from the trc20_sync_cursor materialized view.
/// Contains the timestamp to start syncing from.
#[derive(Debug, Clone)]
pub struct Trc20SyncCursor {
    pub token_name: StablecoinName,
    /// The timestamp (in milliseconds) to start syncing from.
    /// This is either:
    /// - The earliest timestamp of unconfirmed transfers within the last 1 day, or
    /// - The latest timestamp if all recent transfers are confirmed.
    pub cursor_block_timestamp: i64,
    /// Whether there are unconfirmed transfers within the last 1 day.
    pub has_pending_confirmation: bool,
}

#[derive(Debug, Clone)]
/// Get the sync cursor from the materialized view for a token.
///
/// The cursor implements the algorithm:
/// 1. If there are unconfirmed transfers within the last 1 day, return the earliest timestamp
/// 2. Otherwise, return the latest timestamp
/// 3. If no transfers exist, return None
pub struct GetTrc20TokenTransSyncCursor {
    pub token: StablecoinName,
}

impl Processor<GetTrc20TokenTransSyncCursor> for DatabaseProcessor {
    type Output = Option<Trc20SyncCursor>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:GetTrc20TokenTransSyncCursor")]
    async fn process(
        &self,
        query: GetTrc20TokenTransSyncCursor,
    ) -> Result<Option<Trc20SyncCursor>, sqlx::Error> {
        let cursor = sqlx::query_as!(
            Trc20SyncCursor,
            r#"
            SELECT
                token_name as "token_name!: StablecoinName",
                cursor_block_timestamp as "cursor_block_timestamp!",
                has_pending_confirmation as "has_pending_confirmation!"
            FROM trc20_sync_cursor
            WHERE token_name = $1
            "#,
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
pub struct InsertManyTrc20TokenTransfers {
    pub transfers: Vec<Trc20TransferInsert>,
}

impl Processor<InsertManyTrc20TokenTransfers> for DatabaseProcessor {
    type Output = u64;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:InsertManyTrc20TokenTransfers")]
    async fn process(&self, insert: InsertManyTrc20TokenTransfers) -> Result<u64, sqlx::Error> {
        if insert.transfers.is_empty() {
            return Ok(0);
        }

        let mut query_builder = sqlx::QueryBuilder::new(
            "INSERT INTO trc20_token_transfers \
            (token_name, from_address, to_address, txn_hash, value, block_number, block_timestamp) ",
        );

        query_builder.push_values(insert.transfers, |mut b, transfer| {
            b.push_bind(transfer.token_name)
                .push_bind(transfer.from_address)
                .push_bind(transfer.to_address)
                .push_bind(transfer.txn_hash)
                .push_bind(transfer.value)
                .push_bind(transfer.block_number)
                .push_bind(transfer.block_timestamp);
        });

        query_builder.push(" ON CONFLICT (txn_hash) DO NOTHING");

        let result = query_builder.build().execute(&self.pool).await?;
        Ok(result.rows_affected())
    }
}

#[derive(Debug, Clone)]
/// Get unmatched transfers that are waiting for a deposit match.
pub struct GetTrc20TokenTransfersUnmatched {
    pub token: StablecoinName,
}

impl Processor<GetTrc20TokenTransfersUnmatched> for DatabaseProcessor {
    type Output = Vec<Trc20UnmatchedTransfer>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:GetTrc20TokenTransfersUnmatched")]
    async fn process(
        &self,
        query: GetTrc20TokenTransfersUnmatched,
    ) -> Result<Vec<Trc20UnmatchedTransfer>, sqlx::Error> {
        let transfers = sqlx::query_as!(
            Trc20UnmatchedTransfer,
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
            query.token as StablecoinName,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(transfers)
    }
}

#[derive(Debug, Clone)]
/// Get IDs of old unmatched transfers (older than 1 hour) for marking as unknown.
pub struct GetOldUnmatchedTrc20TransferIds {
    pub token: StablecoinName,
}

impl Processor<GetOldUnmatchedTrc20TransferIds> for DatabaseProcessor {
    type Output = Vec<i64>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:GetOldUnmatchedTrc20TransferIds")]
    async fn process(
        &self,
        query: GetOldUnmatchedTrc20TransferIds,
    ) -> Result<Vec<i64>, sqlx::Error> {
        let ids = sqlx::query_scalar!(
            r#"
            SELECT id
            FROM trc20_token_transfers
            WHERE token_name = $1
              AND status = 'waiting_for_match'
              AND blockchain_confirmed = true
              AND created_at < NOW() - INTERVAL '1 hour'
            "#,
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
pub struct MarkTrc20TransfersNoMatchedDeposit {
    pub transfer_ids: Vec<i64>,
}

impl Processor<MarkTrc20TransfersNoMatchedDeposit> for DatabaseProcessor {
    type Output = u64;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:MarkTrc20TransfersNoMatchedDeposit")]
    async fn process(&self, cmd: MarkTrc20TransfersNoMatchedDeposit) -> Result<u64, sqlx::Error> {
        if cmd.transfer_ids.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query!(
            r#"
            UPDATE trc20_token_transfers
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

impl Trc20TokenTransfer {
    /// Mark a transfer as matched with a fulfillment ID within a transaction.
    pub async fn mark_matched_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        transfer_id: i64,
        fulfillment_id: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE trc20_token_transfers
            SET status = 'matched', fulfillment_id = $1
            WHERE id = $2
            "#,
            fulfillment_id,
            transfer_id,
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Mark multiple transfers as matched with their fulfillment IDs in a single query.
    ///
    /// Uses `UNNEST` to batch-update all rows in one SQL statement.
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
            UPDATE trc20_token_transfers AS t
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
