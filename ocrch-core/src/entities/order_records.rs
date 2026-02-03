use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct OrderRecord {
    pub order_id: Uuid,
    pub merchant_order_id: String,
    pub created_at: time::PrimitiveDateTime,
    pub status: OrderStatus,
    pub webhook_success_at: Option<time::PrimitiveDateTime>,
    pub webhook_url: String,
    pub webhook_retry_count: u32,
    pub webhook_last_tried_at: Option<time::PrimitiveDateTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type)]
pub enum OrderStatus {
    Pending,
    Paid,
    Expired,
    Cancelled,
}
