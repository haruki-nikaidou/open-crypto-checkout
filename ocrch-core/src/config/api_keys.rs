//! API keys configuration for blockchain explorer services.

/// API keys for blockchain explorer services.
#[derive(Debug, Clone)]
pub struct ApiKeysConfig {
    /// EtherScan API key (used for all EVM-compatible chains).
    pub etherscan_api_key: String,
    /// TronScan API key (used for the Tron network).
    pub tronscan_api_key: String,
}
