//! Wallet configuration.

use crate::objects::blockchains::{Blockchain, Stablecoin};

/// Wallet configuration for receiving payments.
#[derive(Debug, Clone)]
pub struct WalletConfig {
    /// The blockchain this wallet is on.
    pub blockchain: Blockchain,
    /// The wallet address.
    pub address: String,
    /// List of stablecoins enabled for this wallet.
    pub enabled_coins: Vec<Stablecoin>,
}

impl WalletConfig {
    /// Create a new WalletConfig.
    pub fn new(blockchain: Blockchain, address: String, enabled_coins: Vec<Stablecoin>) -> Self {
        Self {
            blockchain,
            address,
            enabled_coins,
        }
    }

    /// Check if a stablecoin is enabled for this wallet.
    pub fn is_coin_enabled(&self, coin: Stablecoin) -> bool {
        self.enabled_coins.contains(&coin)
    }
}
