use crate::entities::StablecoinName;

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
