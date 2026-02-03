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
    pub block_number: u64,
    pub block_timestamp: u64,
    pub blockchain_confirmed: bool,
    pub created_at: time::PrimitiveDateTime,
    pub status: TransferStatus,
    pub fulfillment_id: Option<i64>,
}
