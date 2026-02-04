//! Merchant configuration.

/// Merchant configuration for API access.
#[derive(Debug, Clone)]
pub struct MerchantConfig {
    /// Unique merchant identifier.
    pub id: String,
    /// Human-readable merchant name.
    pub name: String,
    /// Secret key bytes for HMAC signing.
    pub secret: Box<[u8]>,
    /// URL to send webhook notifications to.
    pub webhook_url: String,
    /// List of allowed origins for CORS (frontend URLs).
    pub allowed_origins: Vec<String>,
}

impl MerchantConfig {
    /// Create a new MerchantConfig.
    pub fn new(
        id: String,
        name: String,
        secret: impl Into<Box<[u8]>>,
        webhook_url: String,
        allowed_origins: Vec<String>,
    ) -> Self {
        Self {
            id,
            name,
            secret: secret.into(),
            webhook_url,
            allowed_origins,
        }
    }

    /// Get the secret key bytes for HMAC signing.
    pub fn secret_bytes(&self) -> &[u8] {
        &self.secret
    }
}
