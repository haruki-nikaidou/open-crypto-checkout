//! Merchant configuration.

/// Merchant configuration for API access.
#[derive(Debug, Clone)]
pub struct MerchantConfig {
    /// Human-readable merchant name.
    pub name: String,
    /// Secret key bytes for HMAC signing.
    pub secret: Box<[u8]>,
    /// List of allowed origins for CORS (frontend URLs).
    pub allowed_origins: Vec<String>,
}

impl MerchantConfig {
    /// Create a new MerchantConfig.
    pub fn new(name: String, secret: impl Into<Box<[u8]>>, allowed_origins: Vec<String>) -> Self {
        Self {
            name,
            secret: secret.into(),
            allowed_origins,
        }
    }

    /// Get the secret key bytes for HMAC signing.
    pub fn secret_bytes(&self) -> &[u8] {
        &self.secret
    }
}
