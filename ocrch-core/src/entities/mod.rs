pub mod erc20_pending_deposit;
pub mod erc20_transfer;
pub mod order_records;
pub mod trc20_pending_deposit;
pub mod trc20_transfer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type)]
#[sqlx(rename_all = "lowercase", type_name = "stablecoin_name")]
pub enum StablecoinName {
    Usdt,
    Usdc,
    Dai,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type)]
#[sqlx(rename_all = "lowercase", type_name = "transfer_status")]
pub enum TransferStatus {
    WaitingForConfirmation,
    FailedToConfirm,
    WaitingForMatch,
    NoMatchedDeposit,
    Matched,
}
