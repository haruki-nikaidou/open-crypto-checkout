use crate::entities::StablecoinName;
use crate::framework::DatabaseProcessor;
use kanau::processor::Processor;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Trc20PendingDeposit {
    pub id: i64,
    pub order: uuid::Uuid,
    pub token_name: StablecoinName,
    pub user_address: Option<String>,
    pub wallet_address: String,
    pub value: rust_decimal::Decimal,
    pub started_at: time::PrimitiveDateTime,
    pub last_scanned_at: time::PrimitiveDateTime,
}

/// A pending deposit for matching operations.
#[derive(Debug, Clone)]
pub struct Trc20PendingDepositMatch {
    pub id: i64,
    pub order_id: uuid::Uuid,
    pub wallet_address: String,
    pub value: rust_decimal::Decimal,
    pub started_at_timestamp: i64,
}

#[derive(Debug, Clone)]
/// Get pending deposits for matching with transfers.
pub struct GetTrc20DepositsForMatching {
    pub token: StablecoinName,
}

impl Processor<GetTrc20DepositsForMatching> for DatabaseProcessor {
    type Output = Vec<Trc20PendingDepositMatch>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:GetTrc20DepositsForMatching")]
    async fn process(
        &self,
        query: GetTrc20DepositsForMatching,
    ) -> Result<Vec<Trc20PendingDepositMatch>, sqlx::Error> {
        let deposits = sqlx::query_as!(
            Trc20PendingDepositMatch,
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
            query.token as StablecoinName,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(deposits)
    }
}

impl Trc20PendingDeposit {
    /// Delete pending deposits for an order except for one (the matched one), within a transaction.
    pub async fn delete_for_order_except_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        order_id: uuid::Uuid,
        except_id: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM trc20_pending_deposits
            WHERE "order" = $1 AND id != $2
            "#,
            order_id,
            except_id,
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Delete all pending deposits for an order within a transaction.
    pub async fn delete_for_order_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        order_id: uuid::Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM trc20_pending_deposits
            WHERE "order" = $1
            "#,
            order_id,
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Delete all pending deposits for multiple orders in a single query.
    ///
    /// Uses `ANY` to batch-delete in one SQL statement.
    pub async fn delete_for_orders_many_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        order_ids: &[uuid::Uuid],
    ) -> Result<u64, sqlx::Error> {
        if order_ids.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query(
            r#"
            DELETE FROM trc20_pending_deposits
            WHERE "order" = ANY($1)
            "#,
        )
        .bind(order_ids)
        .execute(&mut **tx)
        .await?;
        Ok(result.rows_affected())
    }
}
