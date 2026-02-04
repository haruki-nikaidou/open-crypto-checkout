//! Runtime configuration structures.
//!
//! These structs represent the validated configuration used at runtime,
//! with processed/hashed secrets and validated values.

use ocrch_sdk::objects::blockchains::{Blockchain, Stablecoin};
use std::collections::HashMap;
use std::net::SocketAddr;

/// Runtime configuration with validated and processed values.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub server: RuntimeServerConfig,
    pub admin: RuntimeAdminConfig,
    pub merchants: HashMap<String, RuntimeMerchantConfig>,
    pub wallets: Vec<RuntimeWalletConfig>,
}

/// Runtime server configuration.
#[derive(Debug, Clone)]
pub struct RuntimeServerConfig {
    pub listen: SocketAddr,
}

/// Runtime admin configuration with hashed secret.
#[derive(Debug, Clone)]
pub struct RuntimeAdminConfig {
    /// The argon2 hashed admin secret.
    pub secret_hash: String,
}

impl RuntimeAdminConfig {
    /// Verify a plaintext password against the stored hash.
    pub fn verify_secret(&self, plaintext: &str) -> bool {
        use argon2::{Argon2, PasswordHash, PasswordVerifier};

        let Ok(parsed_hash) = PasswordHash::new(&self.secret_hash) else {
            return false;
        };

        Argon2::default()
            .verify_password(plaintext.as_bytes(), &parsed_hash)
            .is_ok()
    }
}

/// Runtime merchant configuration.
#[derive(Debug, Clone)]
pub struct RuntimeMerchantConfig {
    pub id: String,
    pub name: String,
    /// Secret key bytes for HMAC signing.
    pub secret: Box<[u8]>,
    pub webhook_url: String,
    pub allowed_origins: Vec<String>,
}

/// Runtime wallet configuration.
#[derive(Debug, Clone)]
pub struct RuntimeWalletConfig {
    pub blockchain: Blockchain,
    pub address: String,
    pub enabled_coins: Vec<Stablecoin>,
}

impl RuntimeConfig {
    /// Get a merchant by ID.
    pub fn get_merchant(&self, id: &str) -> Option<&RuntimeMerchantConfig> {
        self.merchants.get(id)
    }

    /// Get all wallets for a specific blockchain.
    pub fn wallets_for_blockchain(&self, blockchain: Blockchain) -> Vec<&RuntimeWalletConfig> {
        self.wallets
            .iter()
            .filter(|w| w.blockchain == blockchain)
            .collect()
    }

    /// Get all enabled blockchain-coin pairs.
    pub fn enabled_pairs(&self) -> Vec<(Blockchain, Stablecoin)> {
        self.wallets
            .iter()
            .flat_map(|w| w.enabled_coins.iter().map(move |c| (w.blockchain, *c)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_secret() {
        use argon2::{
            password_hash::{rand_core::OsRng, SaltString},
            Argon2, PasswordHasher,
        };

        let password = "test-password";
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .unwrap()
            .to_string();

        let admin_config = RuntimeAdminConfig { secret_hash: hash };

        assert!(admin_config.verify_secret("test-password"));
        assert!(!admin_config.verify_secret("wrong-password"));
    }
}
