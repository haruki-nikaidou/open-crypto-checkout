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
    pub merchant: Arc<RwLock<MerchantConfig>>,
    /// Wallet configurations for receiving payments.
    pub wallets: Arc<RwLock<Vec<WalletConfig>>>,
}
