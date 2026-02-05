use crate::entities::{StablecoinName, TransferStatus};
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

impl Trc20TokenTransfer {
    /// Get the sync cursor from the materialized view for a token.
    ///
    /// The cursor implements the algorithm:
    /// 1. If there are unconfirmed transfers within the last 1 day, return the earliest timestamp
    /// 2. Otherwise, return the latest timestamp
    /// 3. If no transfers exist, return None
    pub async fn cursor(
        pool: &sqlx::PgPool,
        token_name: StablecoinName,
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
            token_name as StablecoinName,
        )
        .fetch_optional(pool)
        .await?;
        Ok(cursor)
    }

    /// Insert a new transfer. Returns true if a new row was inserted (not a duplicate).
    ///
    /// Uses ON CONFLICT DO NOTHING to ensure idempotency.
    pub async fn insert(
        pool: &sqlx::PgPool,
        transfer: &Trc20TransferInsert,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            INSERT INTO trc20_token_transfers 
            (token_name, from_address, to_address, txn_hash, value, block_number, block_timestamp)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (txn_hash) DO NOTHING
            "#,
            transfer.token_name as StablecoinName,
            transfer.from_address,
            transfer.to_address,
            transfer.txn_hash,
            transfer.value,
            transfer.block_number,
            transfer.block_timestamp,
        )
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Get unmatched transfers that are waiting for a deposit match.
    pub async fn get_unmatched(
        pool: &sqlx::PgPool,
        token: StablecoinName,
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
            token as StablecoinName,
        )
        .fetch_all(pool)
        .await?;
        Ok(transfers)
    }

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

    /// Get IDs of old unmatched transfers (older than 1 hour) for marking as unknown.
    pub async fn get_old_unmatched_ids(
        pool: &sqlx::PgPool,
        token: StablecoinName,
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
            token as StablecoinName,
        )
        .fetch_all(pool)
        .await?;
        Ok(ids)
    }

    /// Mark a transfer as having no matched deposit.
    pub async fn mark_no_matched_deposit(
        pool: &sqlx::PgPool,
        transfer_id: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE trc20_token_transfers
            SET status = 'no_matched_deposit'
            WHERE id = $1
            "#,
            transfer_id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }
}
