//! Configuration types for Open Crypto Checkout.
//!
//! These types represent the validated runtime configuration used by the server
//! and can be shared across crates. The actual config loading/parsing is handled
//! by the server crate.

mod admin;
mod merchant;
mod server;
mod wallet;

pub use admin::AdminConfig;
pub use merchant::MerchantConfig;
pub use server::ServerConfig;
pub use wallet::WalletConfig;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared configuration state with separate locks for each section.
///
/// This allows independent access to different configuration sections
/// without blocking other readers/writers.
#[derive(Clone)]
pub struct SharedConfig {
    /// Server configuration (listen address, etc.).
    pub server: Arc<RwLock<ServerConfig>>,
    /// Admin configuration (authentication).
    pub admin: Arc<RwLock<AdminConfig>>,
    /// Merchant configurations indexed by ID.
    pub merchants: Arc<RwLock<HashMap<String, MerchantConfig>>>,
    /// Wallet configurations for receiving payments.
    pub wallets: Arc<RwLock<Vec<WalletConfig>>>,
}

impl SharedConfig {
    /// Create a new SharedConfig from individual configuration parts.
    pub fn new(
        server: ServerConfig,
        admin: AdminConfig,
        merchants: HashMap<String, MerchantConfig>,
        wallets: Vec<WalletConfig>,
    ) -> Self {
        Self {
            server: Arc::new(RwLock::new(server)),
            admin: Arc::new(RwLock::new(admin)),
            merchants: Arc::new(RwLock::new(merchants)),
            wallets: Arc::new(RwLock::new(wallets)),
        }
    }

    /// Get a read lock on the server configuration.
    pub async fn server(&self) -> tokio::sync::RwLockReadGuard<'_, ServerConfig> {
        self.server.read().await
    }

    /// Get a read lock on the admin configuration.
    pub async fn admin(&self) -> tokio::sync::RwLockReadGuard<'_, AdminConfig> {
        self.admin.read().await
    }

    /// Get a read lock on the merchants configuration.
    pub async fn merchants(
        &self,
    ) -> tokio::sync::RwLockReadGuard<'_, HashMap<String, MerchantConfig>> {
        self.merchants.read().await
    }

    /// Get a read lock on the wallets configuration.
    pub async fn wallets(&self) -> tokio::sync::RwLockReadGuard<'_, Vec<WalletConfig>> {
        self.wallets.read().await
    }

    /// Update the server configuration.
    pub async fn update_server(&self, config: ServerConfig) {
        let mut server = self.server.write().await;
        *server = config;
    }

    /// Update the admin configuration.
    pub async fn update_admin(&self, config: AdminConfig) {
        let mut admin = self.admin.write().await;
        *admin = config;
    }

    /// Update the merchants configuration.
    pub async fn update_merchants(&self, config: HashMap<String, MerchantConfig>) {
        let mut merchants = self.merchants.write().await;
        *merchants = config;
    }

    /// Update the wallets configuration.
    pub async fn update_wallets(&self, config: Vec<WalletConfig>) {
        let mut wallets = self.wallets.write().await;
        *wallets = config;
    }

    /// Update all configuration sections at once.
    pub async fn update_all(
        &self,
        server: ServerConfig,
        admin: AdminConfig,
        merchants: HashMap<String, MerchantConfig>,
        wallets: Vec<WalletConfig>,
    ) {
        // Update in sequence to avoid potential deadlocks
        self.update_server(server).await;
        self.update_admin(admin).await;
        self.update_merchants(merchants).await;
        self.update_wallets(wallets).await;
    }
}
