//! Configuration module for ocrch-server.
//!
//! Handles loading configuration from TOML files, CLI arguments,
//! and environment variables. Also handles admin secret hashing.

pub mod file;
pub mod runtime;

use crate::config::file::{FileConfig, MerchantConfig, WalletConfig};
use crate::config::runtime::{
    RuntimeAdminConfig, RuntimeConfig, RuntimeMerchantConfig, RuntimeServerConfig,
    RuntimeWalletConfig,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during configuration loading.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("failed to parse config file: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("failed to serialize config: {0}")]
    SerializeError(#[from] toml::ser::Error),

    #[error("validation error: {0}")]
    ValidationError(String),

    #[error("password hashing error: {0}")]
    HashError(String),

    #[error("DATABASE_URL environment variable not set")]
    MissingDatabaseUrl,
}

/// Configuration loader that handles the complete loading process.
pub struct ConfigLoader {
    config_path: std::path::PathBuf,
    listen_override: Option<SocketAddr>,
}

impl ConfigLoader {
    /// Create a new config loader.
    pub fn new(config_path: impl AsRef<Path>, listen_override: Option<SocketAddr>) -> Self {
        Self {
            config_path: config_path.as_ref().to_path_buf(),
            listen_override,
        }
    }

    /// Load and process the configuration.
    ///
    /// This will:
    /// 1. Read the TOML file
    /// 2. Apply CLI overrides
    /// 3. Validate the configuration
    /// 4. Hash the admin secret if it's plaintext (and rewrite the file)
    /// 5. Build the runtime configuration
    pub fn load(&self) -> Result<RuntimeConfig, ConfigError> {
        // Read the config file
        let config_content = std::fs::read_to_string(&self.config_path)?;
        let mut file_config: FileConfig = toml::from_str(&config_content)?;

        // Apply CLI overrides
        if let Some(listen) = self.listen_override {
            file_config.server.listen = listen;
        }

        // Validate the configuration
        self.validate(&file_config)?;

        // Hash admin secret if needed and rewrite config
        let secret_hash = if file_config.is_admin_secret_hashed() {
            file_config.admin.secret.clone()
        } else {
            let hash = self.hash_secret(&file_config.admin.secret)?;
            file_config.admin.secret = hash.clone();
            self.rewrite_config(&file_config)?;
            tracing::info!("Admin secret hashed and config file updated");
            hash
        };

        // Build runtime config
        Ok(self.build_runtime_config(file_config, secret_hash))
    }

    fn validate(&self, config: &FileConfig) -> Result<(), ConfigError> {
        // Check for duplicate merchant IDs
        let mut merchant_ids = std::collections::HashSet::new();
        for merchant in &config.merchants {
            if !merchant_ids.insert(&merchant.id) {
                return Err(ConfigError::ValidationError(format!(
                    "duplicate merchant ID: {}",
                    merchant.id
                )));
            }
        }

        // Check that wallets have at least one enabled coin
        for wallet in &config.wallets {
            if wallet.enabled_coins.is_empty() {
                return Err(ConfigError::ValidationError(format!(
                    "wallet {} has no enabled coins",
                    wallet.address
                )));
            }
        }

        Ok(())
    }

    fn hash_secret(&self, plaintext: &str) -> Result<String, ConfigError> {
        use argon2::{
            password_hash::{rand_core::OsRng, SaltString},
            Argon2, PasswordHasher,
        };

        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        argon2
            .hash_password(plaintext.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| ConfigError::HashError(e.to_string()))
    }

    fn rewrite_config(&self, config: &FileConfig) -> Result<(), ConfigError> {
        let toml_string = toml::to_string_pretty(config)?;

        // Write atomically: write to temp file, then rename
        let temp_path = self.config_path.with_extension("toml.tmp");
        std::fs::write(&temp_path, toml_string)?;
        std::fs::rename(&temp_path, &self.config_path)?;

        Ok(())
    }

    fn build_runtime_config(&self, file_config: FileConfig, secret_hash: String) -> RuntimeConfig {
        let merchants: HashMap<String, RuntimeMerchantConfig> = file_config
            .merchants
            .into_iter()
            .map(|m| {
                (
                    m.id.clone(),
                    convert_merchant(m),
                )
            })
            .collect();

        let wallets: Vec<RuntimeWalletConfig> = file_config
            .wallets
            .into_iter()
            .map(convert_wallet)
            .collect();

        RuntimeConfig {
            server: RuntimeServerConfig {
                listen: file_config.server.listen,
            },
            admin: RuntimeAdminConfig { secret_hash },
            merchants,
            wallets,
        }
    }
}

fn convert_merchant(m: MerchantConfig) -> RuntimeMerchantConfig {
    RuntimeMerchantConfig {
        id: m.id,
        name: m.name,
        secret: m.secret.into_bytes().into_boxed_slice(),
        webhook_url: m.webhook_url,
        allowed_origins: m.allowed_origins,
    }
}

fn convert_wallet(w: WalletConfig) -> RuntimeWalletConfig {
    RuntimeWalletConfig {
        blockchain: w.blockchain,
        address: w.address,
        enabled_coins: w.enabled_coins,
    }
}

/// Get the database URL from the environment.
pub fn get_database_url() -> Result<String, ConfigError> {
    std::env::var("DATABASE_URL").map_err(|_| ConfigError::MissingDatabaseUrl)
}
