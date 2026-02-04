//! TOML file configuration structures.
//!
//! These structs directly map to the `ocrch-config.toml` file format.

use ocrch_sdk::objects::blockchains::{Blockchain, Stablecoin};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Root configuration structure as read from the TOML file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileConfig {
    pub server: ServerConfig,
    pub admin: AdminConfig,
    pub merchant: MerchantConfig,
    #[serde(default)]
    pub wallets: Vec<WalletConfig>,
}

/// Server configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// The address and port to listen on (e.g., "0.0.0.0:8080").
    #[serde(default = "default_listen_addr")]
    pub listen: SocketAddr,
}

fn default_listen_addr() -> SocketAddr {
    "0.0.0.0:8080".parse().expect("valid default address")
}

/// Admin configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminConfig {
    /// The admin secret. If this is plaintext (doesn't start with `$argon2`),
    /// it will be hashed and the config file will be rewritten.
    pub secret: String,
}

/// Merchant configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantConfig {
    /// Human-readable merchant name.
    pub name: String,
    /// Secret key for signing API requests.
    pub secret: String,
    /// List of allowed origins for CORS (frontend URLs).
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}

/// Wallet configuration for receiving payments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConfig {
    /// The blockchain this wallet is on.
    pub blockchain: Blockchain,
    /// The wallet address.
    pub address: String,
    /// List of stablecoins enabled for this wallet.
    pub enabled_coins: Vec<Stablecoin>,
}

impl FileConfig {
    /// Check if the admin secret is already hashed (argon2 format).
    pub fn is_admin_secret_hashed(&self) -> bool {
        self.admin.secret.starts_with("$argon2")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_parsing() {
        let toml_str = r#"
[server]
listen = "127.0.0.1:3000"

[admin]
secret = "test-secret"

[merchants]
name = "Test Store"
secret = "secret123"
webhook_url = "https://example.com/webhook"
allowed_origins = ["https://checkout.example.com"]

[[wallets]]
blockchain = "eth"
address = "0x1234567890abcdef"
enabled_coins = ["USDT", "USDC"]
"#;
        let config: FileConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.server.listen.port(), 3000);
        assert_eq!(config.merchant.name, "Test Store");
        assert_eq!(config.wallets.len(), 1);
        assert!(!config.is_admin_secret_hashed());
    }

    #[test]
    fn test_hashed_secret_detection() {
        let config = FileConfig {
            server: ServerConfig {
                listen: default_listen_addr(),
            },
            admin: AdminConfig {
                secret: "$argon2id$v=19$m=19456,t=2,p=1$abc123".to_string(),
            },
            merchant: MerchantConfig {
                name: "Test Store".to_string(),
                secret: "secret123".to_string(),
                allowed_origins: vec![],
            },
            wallets: vec![],
        };
        assert!(config.is_admin_secret_hashed());
    }
}
