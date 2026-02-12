//! Configuration module for ocrch-server.
//!
//! Handles loading configuration from TOML files, CLI arguments,
//! and environment variables. Also handles admin secret hashing.

pub mod file;
pub mod runtime;

use crate::config::file::{
    FileConfig, MerchantConfig as FileMerchantConfig, WalletConfig as FileWalletConfig,
};
use crate::config::runtime::{
    AdminConfig, ApiKeysConfig, MerchantConfig, ServerConfig, SharedConfig, WalletConfig,
};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

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

/// Loaded configuration result containing all parts.
pub struct LoadedConfig {
    pub server: ServerConfig,
    pub admin: AdminConfig,
    pub merchant: MerchantConfig,
    pub wallets: Vec<WalletConfig>,
    pub api_keys: ApiKeysConfig,
}

impl LoadedConfig {
    /// Convert into a SharedConfig with Arc<RwLock<T>> wrappers.
    pub fn into_shared(self) -> SharedConfig {
        SharedConfig {
            server: Arc::new(RwLock::new(self.server)),
            admin: Arc::new(RwLock::new(self.admin)),
            merchant: Arc::new(RwLock::new(self.merchant)),
            wallets: Arc::new(RwLock::new(self.wallets)),
            api_keys: Arc::new(RwLock::new(self.api_keys)),
        }
    }
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
    /// 5. Build the loaded configuration
    pub fn load(&self) -> Result<LoadedConfig, ConfigError> {
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

        // Build the config parts
        Ok(self.build_loaded_config(file_config, secret_hash))
    }

    /// Reload the configuration (used during SIGHUP).
    ///
    /// Returns a LoadedConfig that can be used to update individual parts
    /// of a SharedConfig.
    pub fn reload(&self) -> Result<LoadedConfig, ConfigError> {
        self.load()
    }

    fn validate(&self, config: &FileConfig) -> Result<(), ConfigError> {
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
            Argon2, PasswordHasher,
            password_hash::{SaltString, rand_core::OsRng},
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

    fn build_loaded_config(&self, file_config: FileConfig, secret_hash: String) -> LoadedConfig {
        let wallets: Vec<WalletConfig> = file_config
            .wallets
            .into_iter()
            .map(convert_wallet)
            .collect();

        LoadedConfig {
            server: ServerConfig {
                listen: file_config.server.listen,
            },
            admin: AdminConfig::new(secret_hash),
            merchant: convert_merchant(file_config.merchant),
            wallets,
            api_keys: ApiKeysConfig {
                etherscan_api_key: file_config.api_keys.etherscan_api_key,
                tronscan_api_key: file_config.api_keys.tronscan_api_key,
            },
        }
    }
}

fn convert_merchant(m: FileMerchantConfig) -> MerchantConfig {
    MerchantConfig::new(
        m.name,
        m.secret.into_bytes().into_boxed_slice(),
        m.allowed_origins,
    )
}

fn convert_wallet(w: FileWalletConfig) -> WalletConfig {
    WalletConfig {
        blockchain: w.blockchain,
        address: w.address,
        enabled_coins: w.enabled_coins,
        starting_tx: w.starting_tx,
    }
}

/// Get the database URL from the environment.
pub fn get_database_url() -> Result<String, ConfigError> {
    std::env::var("DATABASE_URL").map_err(|_| ConfigError::MissingDatabaseUrl)
}
