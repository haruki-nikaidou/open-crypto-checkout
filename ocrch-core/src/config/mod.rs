//! Configuration types for Open Crypto Checkout.
//!
//! These types represent the validated runtime configuration used by the server
//! and can be shared across crates. The actual config loading/parsing is handled
//! by the server crate.

mod admin;
mod api_keys;
mod config_store;
mod merchant;
mod server;
mod wallet;

pub use admin::AdminConfig;
pub use api_keys::ApiKeysConfig;
pub use config_store::{ConfigStore, ConfigWatcher};
pub use merchant::MerchantConfig;
pub use server::ServerConfig;
pub use wallet::WalletConfig;

/// Owns the config stores for each configuration section, keeping them alive
/// for the duration of the application. Clone it cheaply to share handles.
#[derive(Clone)]
pub struct SharedConfig {
    /// Server configuration (listen address, etc.).
    pub server: ConfigStore<ServerConfig>,
    /// Admin configuration (authentication).
    pub admin: ConfigStore<AdminConfig>,
    /// Merchant configurations indexed by ID.
    pub merchant: ConfigStore<MerchantConfig>,
    /// Wallet configurations for receiving payments.
    pub wallets: ConfigStore<Vec<WalletConfig>>,
    /// API keys for blockchain explorer services.
    pub api_keys: ConfigStore<ApiKeysConfig>,
}
