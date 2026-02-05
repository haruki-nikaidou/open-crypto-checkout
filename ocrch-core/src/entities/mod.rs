pub mod erc20_pending_deposit;
pub mod erc20_transfer;
pub mod order_records;
pub mod trc20_pending_deposit;
pub mod trc20_transfer;

use ocrch_sdk::objects::{Stablecoin as SdkStablecoin, TransferStatus as SdkTransferStatus};

/// Stablecoin name for database operations.
///
/// This is the sqlx::Type version. For API/DTO use, see `ocrch_sdk::objects::Stablecoin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type)]
#[sqlx(rename_all = "lowercase", type_name = "stablecoin_name")]
pub enum StablecoinName {
    Usdt,
    Usdc,
    Dai,
}

impl From<StablecoinName> for SdkStablecoin {
    fn from(value: StablecoinName) -> Self {
        match value {
            StablecoinName::Usdt => SdkStablecoin::Usdt,
            StablecoinName::Usdc => SdkStablecoin::Usdc,
            StablecoinName::Dai => SdkStablecoin::Dai,
        }
    }
}

impl From<SdkStablecoin> for StablecoinName {
    fn from(value: SdkStablecoin) -> Self {
        match value {
            SdkStablecoin::Usdt => StablecoinName::Usdt,
            SdkStablecoin::Usdc => StablecoinName::Usdc,
            SdkStablecoin::Dai => StablecoinName::Dai,
        }
    }
}

/// Transfer status for database operations.
///
/// This is the sqlx::Type version. For API/DTO use, see `ocrch_sdk::objects::TransferStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type)]
#[sqlx(rename_all = "lowercase", type_name = "transfer_status")]
pub enum TransferStatus {
    WaitingForConfirmation,
    FailedToConfirm,
    WaitingForMatch,
    NoMatchedDeposit,
    Matched,
}

impl From<TransferStatus> for SdkTransferStatus {
    fn from(value: TransferStatus) -> Self {
        match value {
            TransferStatus::WaitingForConfirmation => SdkTransferStatus::WaitingForConfirmation,
            TransferStatus::FailedToConfirm => SdkTransferStatus::FailedToConfirm,
            TransferStatus::WaitingForMatch => SdkTransferStatus::WaitingForMatch,
            TransferStatus::NoMatchedDeposit => SdkTransferStatus::NoMatchedDeposit,
            TransferStatus::Matched => SdkTransferStatus::Matched,
        }
    }
}

impl From<SdkTransferStatus> for TransferStatus {
    fn from(value: SdkTransferStatus) -> Self {
        match value {
            SdkTransferStatus::WaitingForConfirmation => TransferStatus::WaitingForConfirmation,
            SdkTransferStatus::FailedToConfirm => TransferStatus::FailedToConfirm,
            SdkTransferStatus::WaitingForMatch => TransferStatus::WaitingForMatch,
            SdkTransferStatus::NoMatchedDeposit => TransferStatus::NoMatchedDeposit,
            SdkTransferStatus::Matched => TransferStatus::Matched,
        }
    }
}
