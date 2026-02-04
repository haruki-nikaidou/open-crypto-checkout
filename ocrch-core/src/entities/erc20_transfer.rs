use crate::entities::erc20_pending_deposit::EtherScanChain;
use crate::entities::{StablecoinName, TransferStatus};

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

impl Erc20TokenTransfer {
    pub async fn cursor(
        pool: &sqlx::PgPool,
        chain: EtherScanChain,
        token_name: StablecoinName,
    ) -> Result<Option<Self>, sqlx::Error> {
        let transfer = sqlx::query_as!(
            Self,
            r#"
            SELECT
            id,
            token_name as "token_name: StablecoinName",
            chain as "chain: EtherScanChain",
            from_address,
            to_address,
            txn_hash,
            value,
            block_number,
            block_timestamp,
            blockchain_confirmed,
            created_at,
            status as "status: TransferStatus",
            fulfillment_id
            FROM erc20_token_transfers WHERE chain = $1 AND token_name = $2
            "#,
            chain as EtherScanChain,
            token_name as StablecoinName,
        )
        .fetch_optional(pool)
        .await?;
        Ok(transfer)
    }
}
