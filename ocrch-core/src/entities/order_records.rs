use ocrch_sdk::objects::OrderStatus as SdkOrderStatus;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct OrderRecord {
    pub order_id: Uuid,
    pub merchant_order_id: String,
    pub amount: rust_decimal::Decimal,
    pub created_at: time::PrimitiveDateTime,
    pub status: OrderStatus,
    pub webhook_success_at: Option<time::PrimitiveDateTime>,
    pub webhook_url: String,
    pub webhook_retry_count: i32,
    pub webhook_last_tried_at: Option<time::PrimitiveDateTime>,
}

/// Order status for database operations.
///
/// This is the sqlx::Type version. For API/DTO use, see `ocrch_sdk::objects::OrderStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type)]
#[sqlx(rename_all = "lowercase", type_name = "order_status")]
pub enum OrderStatus {
    Pending,
    Paid,
    Expired,
    Cancelled,
}

impl From<OrderStatus> for SdkOrderStatus {
    fn from(value: OrderStatus) -> Self {
        match value {
            OrderStatus::Pending => SdkOrderStatus::Pending,
            OrderStatus::Paid => SdkOrderStatus::Paid,
            OrderStatus::Expired => SdkOrderStatus::Expired,
            OrderStatus::Cancelled => SdkOrderStatus::Cancelled,
        }
    }
}

impl From<SdkOrderStatus> for OrderStatus {
    fn from(value: SdkOrderStatus) -> Self {
        match value {
            SdkOrderStatus::Pending => OrderStatus::Pending,
            SdkOrderStatus::Paid => OrderStatus::Paid,
            SdkOrderStatus::Expired => OrderStatus::Expired,
            SdkOrderStatus::Cancelled => OrderStatus::Cancelled,
        }
    }
}

/// Data returned when fetching orders for webhook retry.
#[derive(Debug, Clone)]
pub struct OrderForWebhookRetry {
    pub order_id: Uuid,
    pub merchant_order_id: String,
    pub amount: rust_decimal::Decimal,
    pub status: OrderStatus,
    pub webhook_url: String,
    pub webhook_retry_count: i32,
}

impl OrderRecord {
    /// Get an order by its ID.
    pub async fn get_by_id(
        pool: &sqlx::PgPool,
        order_id: Uuid,
    ) -> Result<Option<OrderRecord>, sqlx::Error> {
        let order = sqlx::query_as!(
            OrderRecord,
            r#"
            SELECT 
                order_id,
                merchant_order_id,
                amount,
                created_at,
                status as "status: OrderStatus",
                webhook_success_at,
                webhook_url,
                webhook_retry_count,
                webhook_last_tried_at
            FROM order_records
            WHERE order_id = $1
            "#,
            order_id,
        )
        .fetch_optional(pool)
        .await?;
        Ok(order)
    }

    /// Mark a webhook as successfully delivered.
    pub async fn mark_webhook_success(
        pool: &sqlx::PgPool,
        order_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE order_records
            SET webhook_success_at = NOW()
            WHERE order_id = $1
            "#,
            order_id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Increment the retry count for a failed webhook delivery.
    pub async fn increment_webhook_retry_count(
        pool: &sqlx::PgPool,
        order_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE order_records
            SET 
                webhook_retry_count = webhook_retry_count + 1,
                webhook_last_tried_at = NOW()
            WHERE order_id = $1
            "#,
            order_id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Get orders that need webhook retry based on exponential backoff.
    ///
    /// Returns orders that:
    /// - Have a status change (not pending)
    /// - Haven't been successfully delivered
    /// - Haven't exceeded max retries
    /// - Are due for retry based on exponential backoff (2^retry_count seconds)
    pub async fn get_orders_for_webhook_retry(
        pool: &sqlx::PgPool,
        max_retry_count: i32,
        limit: i64,
    ) -> Result<Vec<OrderForWebhookRetry>, sqlx::Error> {
        let orders = sqlx::query_as!(
            OrderForWebhookRetry,
            r#"
            SELECT 
                order_id,
                merchant_order_id,
                amount,
                status as "status: OrderStatus",
                webhook_url,
                webhook_retry_count
            FROM order_records
            WHERE webhook_success_at IS NULL
              AND status != 'pending'
              AND webhook_retry_count < $1
              AND (
                webhook_last_tried_at IS NULL
                OR webhook_last_tried_at + (POWER(2, webhook_retry_count) || ' seconds')::interval < NOW()
              )
            LIMIT $2
            "#,
            max_retry_count,
            limit,
        )
        .fetch_all(pool)
        .await?;
        Ok(orders)
    }

    /// Update the status of an order.
    pub async fn update_status(
        pool: &sqlx::PgPool,
        order_id: Uuid,
        status: OrderStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE order_records
            SET status = $1
            WHERE order_id = $2
            "#,
            status as OrderStatus,
            order_id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Update the status of an order within a transaction.
    pub async fn update_status_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        order_id: Uuid,
        status: OrderStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE order_records
            SET status = $1
            WHERE order_id = $2
            "#,
            status as OrderStatus,
            order_id,
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}
