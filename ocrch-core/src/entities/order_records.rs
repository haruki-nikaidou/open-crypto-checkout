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
    pub webhook_retry_count: u32,
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
