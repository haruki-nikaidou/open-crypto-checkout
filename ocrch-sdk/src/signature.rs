//! Signature algorithm and verification for all Ocrch APIs.
//!
//! Every Ocrch API endpoint uses HMAC-SHA256 signatures defined in this
//! module.  The wire format for the header is:
//!
//! ```text
//! Ocrch-Signature: {unix_timestamp}.{base64_signature}
//! ```
//!
//! Two signing schemes exist:
//!
//! * **Body signing** (Service API, Webhook API):
//!   `HMAC-SHA256("{timestamp}.{json_body}", secret)`
//!
//! * **URL signing** (User API):
//!   `HMAC-SHA256("{full_url}.{timestamp}", secret)`

/// Header name for the HMAC signature.
pub const SIGNATURE_HEADER: &str = "Ocrch-Signature";

/// Header name carrying the signed frontend URL (User API).
pub const SIGNED_URL_HEADER: &str = "Ocrch-Signed-Url";

/// Header name for admin API authentication (plaintext secret).
pub const ADMIN_AUTH_HEADER: &str = "Ocrch-Admin-Authorization";

/// Maximum allowed age of a signature (in seconds).
pub const MAX_SIGNATURE_AGE: i64 = 5 * 60;

/// Marker trait for types that can participate in body signing via
/// [`SignedObject`].
pub trait Signature: for<'de> serde::Deserialize<'de> + serde::Serialize {}

/// Errors produced by signature operations.
#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    #[error("invalid header format")]
    InvalidFormat,
    #[error("invalid base64 encoding")]
    InvalidBase64,
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid signature")]
    SignatureMismatch,
    #[error("signature expired")]
    Expired,
}

impl From<ring::error::Unspecified> for SignatureError {
    fn from(_: ring::error::Unspecified) -> Self {
        Self::SignatureMismatch
    }
}

// ---------------------------------------------------------------------------
// SignedObject — body signing
// ---------------------------------------------------------------------------

/// A signed API body carrying its typed payload, timestamp, raw JSON, and
/// HMAC-SHA256 signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedObject<T: Signature> {
    pub body: T,
    pub timestamp: i64,
    pub json: String,
    pub signature: Box<[u8]>,
}

impl<T: Signature> SignedObject<T> {
    /// Create a new signed object.
    ///
    /// Serializes `body` to JSON, computes
    /// `HMAC-SHA256("{timestamp}.{json}", key)`, and returns the assembled
    /// [`SignedObject`].
    pub fn new(body: T, key: &[u8]) -> Result<Self, serde_json::Error> {
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let json = serde_json::to_string(&body)?;
        let data = format!("{now}.{json}");
        let signature = ring::hmac::sign(
            &ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key),
            data.as_bytes(),
        );
        let signature = signature.as_ref().to_owned().into_boxed_slice();
        Ok(Self {
            body,
            timestamp: now,
            json,
            signature,
        })
    }

    /// Reconstruct a [`SignedObject`] from a raw `Ocrch-Signature` header
    /// value and the JSON request body string.
    ///
    /// This parses the header and deserializes the body but does **not**
    /// verify the HMAC — call [`verify`](Self::verify) for that.
    pub fn from_header_and_body(
        header_value: &str,
        body_json: String,
    ) -> Result<Self, SignatureError> {
        let (timestamp, signature) = parse_signature_header(header_value)?;
        let body: T = serde_json::from_str(&body_json)?;
        Ok(Self {
            body,
            timestamp,
            json: body_json,
            signature,
        })
    }

    /// Verify the HMAC signature and timestamp freshness, consuming `self`
    /// and returning the authenticated payload.
    pub fn verify(self, key: &[u8]) -> Result<T, SignatureError> {
        let data = format!("{}.{}", self.timestamp, self.json);
        ring::hmac::verify(
            &ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key),
            data.as_bytes(),
            self.signature.as_ref(),
        )?;
        check_timestamp(self.timestamp)?;
        Ok(self.body)
    }

    /// Format the full `Ocrch-Signature` header value (`{timestamp}.{b64}`).
    pub fn to_header(&self) -> String {
        format_signature_header(self.timestamp, &self.signature)
    }

    /// Base64-encode the raw signature bytes (without the timestamp prefix).
    pub fn stringify_signature(&self) -> String {
        fast32::base64::RFC4648_NOPAD.encode(&self.signature)
    }
}

// ---------------------------------------------------------------------------
// Header parsing / formatting
// ---------------------------------------------------------------------------

/// Parse an `Ocrch-Signature` header value (`{timestamp}.{base64}`) into
/// `(timestamp, raw_signature_bytes)`.
pub fn parse_signature_header(value: &str) -> Result<(i64, Box<[u8]>), SignatureError> {
    let dot_pos = value.find('.').ok_or(SignatureError::InvalidFormat)?;
    let timestamp: i64 = value[..dot_pos]
        .parse()
        .map_err(|_| SignatureError::InvalidFormat)?;
    let signature_bytes = fast32::base64::RFC4648_NOPAD
        .decode_str(&value[dot_pos + 1..])
        .map_err(|_| SignatureError::InvalidBase64)?
        .into_boxed_slice();
    Ok((timestamp, signature_bytes))
}

/// Format a `{timestamp}.{base64}` header value from its parts.
pub fn format_signature_header(timestamp: i64, signature: &[u8]) -> String {
    format!(
        "{}.{}",
        timestamp,
        fast32::base64::RFC4648_NOPAD.encode(signature)
    )
}

// ---------------------------------------------------------------------------
// Timestamp validation
// ---------------------------------------------------------------------------

/// Check that a signature timestamp is within [`MAX_SIGNATURE_AGE`].
pub fn check_timestamp(timestamp: i64) -> Result<(), SignatureError> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    if now - timestamp > MAX_SIGNATURE_AGE {
        return Err(SignatureError::Expired);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// URL signing (User API)
// ---------------------------------------------------------------------------

/// Sign a frontend URL: `HMAC-SHA256("{url}.{timestamp}", key)`.
///
/// Returns the formatted `Ocrch-Signature` header value.
pub fn sign_url(url: &str, key: &[u8]) -> String {
    let timestamp = time::OffsetDateTime::now_utc().unix_timestamp();
    let data = format!("{url}.{timestamp}");
    let sig = ring::hmac::sign(
        &ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key),
        data.as_bytes(),
    );
    format_signature_header(timestamp, sig.as_ref())
}

/// Verify a signed frontend URL.
///
/// Checks `HMAC-SHA256("{url}.{timestamp}", key)` and timestamp freshness.
pub fn verify_url(
    url: &str,
    timestamp: i64,
    signature: &[u8],
    key: &[u8],
) -> Result<(), SignatureError> {
    let data = format!("{url}.{timestamp}");
    ring::hmac::verify(
        &ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key),
        data.as_bytes(),
        signature,
    )?;
    check_timestamp(timestamp)?;
    Ok(())
}
