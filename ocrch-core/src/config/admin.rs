//! Admin configuration.

use argon2::{Argon2, PasswordHash, PasswordVerifier};

/// Admin configuration with hashed secret.
#[derive(Debug, Clone)]
pub struct AdminConfig {
    /// The argon2 hashed admin secret.
    pub secret_hash: String,
}

impl AdminConfig {
    /// Create a new AdminConfig with the given hashed secret.
    pub fn new(secret_hash: String) -> Self {
        Self { secret_hash }
    }

    /// Verify a plaintext password against the stored hash.
    ///
    /// Returns `true` if the password matches, `false` otherwise.
    pub fn verify_secret(&self, plaintext: &str) -> bool {
        let Ok(parsed_hash) = PasswordHash::new(&self.secret_hash) else {
            return false;
        };

        Argon2::default()
            .verify_password(plaintext.as_bytes(), &parsed_hash)
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argon2::{
        Argon2, PasswordHasher,
        password_hash::{SaltString, rand_core::OsRng},
    };

    #[test]
    fn test_verify_secret() {
        let password = "test-password";
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .unwrap()
            .to_string();

        let admin_config = AdminConfig::new(hash);

        assert!(admin_config.verify_secret("test-password"));
        assert!(!admin_config.verify_secret("wrong-password"));
    }
}
