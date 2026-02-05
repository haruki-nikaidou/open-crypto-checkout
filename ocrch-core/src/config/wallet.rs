//! Wallet configuration.

use ocrch_sdk::objects::blockchains::{Blockchain, Stablecoin};

/// Wallet configuration for receiving payments.
#[derive(Debug, Clone)]
pub struct WalletConfig {
    /// The blockchain this wallet is on.
    pub blockchain: Blockchain,
    /// The wallet address.
    pub address: String,
    /// List of stablecoins enabled for this wallet.
    pub enabled_coins: Vec<Stablecoin>,
    /// Optional starting transaction hash for initial sync.
    /// When no transfers exist in the database, sync will start from this
    /// transaction's block (ERC-20) or timestamp (TRC-20) instead of from the beginning.
    pub starting_tx: Option<String>,
}
