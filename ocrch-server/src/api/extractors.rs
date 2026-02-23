//! Custom Axum extractors for request processing.
//!
//! Provides:
//! - `SignedBody<T>` — verifies the `Ocrch-Signature` header against a signed JSON body
//!   (used by the Service API).
//! - `VerifiedUrl` — verifies the `Ocrch-Signature` header against a signed frontend URL
//!   carried in the `Ocrch-Signed-Url` header (used by the User API).

use axum::{
    extract::{FromRequest, FromRequestParts, Request},
    http::{HeaderMap, StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use ocrch_sdk::objects::{Signature, SignedObject};

use crate::state::AppState;

/// Header name for the HMAC signature.
const SIGNATURE_HEADER: &str = "Ocrch-Signature";

/// Header name for the signed frontend URL (User API).
const SIGNED_URL_HEADER: &str = "Ocrch-Signed-Url";

/// Maximum age (in seconds) for a signed URL timestamp.
const MAX_SIGNATURE_AGE: i64 = 5 * 60;

/// An Axum extractor that verifies the `Ocrch-Signature` header and
/// deserializes + authenticates the JSON request body.
///
/// # Header format
///
/// ```text
/// Ocrch-Signature: {unix_timestamp}.{base64_signature}
/// ```
///
/// The signature is computed as `HMAC-SHA256("{timestamp}.{json_body}", merchant_secret)`.
pub struct SignedBody<T: Signature>(pub T);

/// Errors that can occur during signature verification.
#[derive(Debug)]
pub enum SignedBodyError {
    /// The `Ocrch-Signature` header is missing.
    MissingHeader,
    /// The header value is not valid UTF-8 or has wrong format.
    InvalidHeader,
    /// Base64 decoding of the signature failed.
    InvalidBase64,
    /// Failed to read request body.
    BodyReadError,
    /// Failed to deserialize JSON body.
    JsonError(serde_json::Error),
    /// HMAC verification failed or timestamp too old.
    VerificationFailed,
}

impl IntoResponse for SignedBodyError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            SignedBodyError::MissingHeader => {
                (StatusCode::UNAUTHORIZED, "missing Ocrch-Signature header")
            }
            SignedBodyError::InvalidHeader => (
                StatusCode::BAD_REQUEST,
                "invalid Ocrch-Signature header format",
            ),
            SignedBodyError::InvalidBase64 => {
                (StatusCode::BAD_REQUEST, "invalid signature encoding")
            }
            SignedBodyError::BodyReadError => {
                (StatusCode::BAD_REQUEST, "failed to read request body")
            }
            SignedBodyError::JsonError(_) => (StatusCode::BAD_REQUEST, "invalid JSON body"),
            SignedBodyError::VerificationFailed => {
                (StatusCode::UNAUTHORIZED, "signature verification failed")
            }
        };
        (status, message).into_response()
    }
}

/// Parse the `Ocrch-Signature` header into (timestamp, raw_signature_bytes).
fn parse_signature_header(headers: &HeaderMap) -> Result<(i64, Box<[u8]>), SignedBodyError> {
    let header_value = headers
        .get(SIGNATURE_HEADER)
        .ok_or(SignedBodyError::MissingHeader)?
        .to_str()
        .map_err(|_| SignedBodyError::InvalidHeader)?;

    // Format: "{timestamp}.{base64_signature}"
    let dot_pos = header_value
        .find('.')
        .ok_or(SignedBodyError::InvalidHeader)?;

    let timestamp_str = &header_value[..dot_pos];
    let signature_b64 = &header_value[dot_pos + 1..];

    let timestamp: i64 = timestamp_str
        .parse()
        .map_err(|_| SignedBodyError::InvalidHeader)?;

    let signature_bytes = fast32::base64::RFC4648_NOPAD
        .decode_str(signature_b64)
        .map_err(|_| SignedBodyError::InvalidBase64)?
        .into_boxed_slice();

    Ok((timestamp, signature_bytes))
}

impl<T: Signature + Send> FromRequest<AppState> for SignedBody<T> {
    type Rejection = SignedBodyError;

    async fn from_request(req: Request, state: &AppState) -> Result<Self, Self::Rejection> {
        // 1. Parse the signature header before consuming the body
        let (timestamp, signature_bytes) = parse_signature_header(req.headers())?;

        // 2. Read the raw body bytes
        let body_bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024)
            .await
            .map_err(|e| SignedBodyError::BodyReadError)?;

        let json = String::from_utf8(body_bytes.to_vec())
            .map_err(|e| SignedBodyError::BodyReadError)?;

        // 3. Deserialize the body to get the typed value
        let body: T =
            serde_json::from_str(&json).map_err(|e| SignedBodyError::JsonError(e))?;

        // 4. Reconstruct SignedObject and verify against merchant secret
        //    (done in a block to avoid holding the RwLock guard across an await)
        let merchant = state.config.merchant.read().await;
        let secret = merchant.secret_bytes();

        let signed = SignedObject {
            body,
            timestamp,
            json,
            signature: signature_bytes,
        };

        let verified_body = signed
            .verify(secret)
            .map_err(|e| SignedBodyError::VerificationFailed)?;

        drop(merchant);

        Ok(SignedBody(verified_body))
    }
}

// ---------------------------------------------------------------------------
// VerifiedUrl — User API authentication via signed frontend URL
// ---------------------------------------------------------------------------

/// An Axum extractor that verifies the `Ocrch-Signature` header against
/// a signed frontend URL from the `Ocrch-Signed-Url` header.
///
/// # Header format
///
/// ```text
/// Ocrch-Signature:  {unix_timestamp}.{base64_signature}
/// Ocrch-Signed-Url: https://checkout.example.com/pay?order_id=...
/// ```
///
/// The signature is computed as
/// `HMAC-SHA256("{full_url}.{timestamp}", merchant_secret)`.
///
/// Implements `FromRequestParts` so it can be combined with `Json<T>`,
/// `Path<T>`, etc.
pub struct VerifiedUrl;

/// Errors returned by the [`VerifiedUrl`] extractor.
#[derive(Debug)]
pub enum VerifiedUrlError {
    /// The `Ocrch-Signature` header is missing.
    MissingSignature,
    /// The `Ocrch-Signed-Url` header is missing.
    MissingUrl,
    /// A header value is malformed.
    InvalidHeader,
    /// Base64 decoding of the signature failed.
    InvalidBase64,
    /// HMAC verification failed.
    SignatureMismatch,
    /// The timestamp is too old.
    TimestampTooOld,
    /// The URL origin is not in the merchant's allowed_origins list.
    OriginNotAllowed,
}

impl IntoResponse for VerifiedUrlError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            VerifiedUrlError::MissingSignature => {
                (StatusCode::UNAUTHORIZED, "missing Ocrch-Signature header")
            }
            VerifiedUrlError::MissingUrl => {
                (StatusCode::BAD_REQUEST, "missing Ocrch-Signed-Url header")
            }
            VerifiedUrlError::InvalidHeader => (
                StatusCode::BAD_REQUEST,
                "invalid header format",
            ),
            VerifiedUrlError::InvalidBase64 => {
                (StatusCode::BAD_REQUEST, "invalid signature encoding")
            }
            VerifiedUrlError::SignatureMismatch => {
                (StatusCode::UNAUTHORIZED, "signature verification failed")
            }
            VerifiedUrlError::TimestampTooOld => {
                (StatusCode::UNAUTHORIZED, "signature expired")
            }
            VerifiedUrlError::OriginNotAllowed => {
                (StatusCode::FORBIDDEN, "origin not allowed")
            }
        };
        (status, message).into_response()
    }
}

impl FromRequestParts<AppState> for VerifiedUrl {
    type Rejection = VerifiedUrlError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // 1. Parse the Ocrch-Signature header → (timestamp, signature_bytes)
        let sig_value = parts
            .headers
            .get(SIGNATURE_HEADER)
            .ok_or(VerifiedUrlError::MissingSignature)?
            .to_str()
            .map_err(|_| VerifiedUrlError::InvalidHeader)?;

        let dot_pos = sig_value
            .find('.')
            .ok_or(VerifiedUrlError::InvalidHeader)?;

        let timestamp: i64 = sig_value[..dot_pos]
            .parse()
            .map_err(|_| VerifiedUrlError::InvalidHeader)?;

        let signature_bytes = fast32::base64::RFC4648_NOPAD
            .decode_str(&sig_value[dot_pos + 1..])
            .map_err(|_| VerifiedUrlError::InvalidBase64)?;

        // 2. Read the Ocrch-Signed-Url header
        let signed_url = parts
            .headers
            .get(SIGNED_URL_HEADER)
            .ok_or(VerifiedUrlError::MissingUrl)?
            .to_str()
            .map_err(|_| VerifiedUrlError::InvalidHeader)?;

        // 3. Verify HMAC: data = "{url}.{timestamp}"
        let data = format!("{signed_url}.{timestamp}");
        let merchant = state.config.merchant.read().await;
        let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, merchant.secret_bytes());

        ring::hmac::verify(&key, data.as_bytes(), &signature_bytes)
            .map_err(|_| VerifiedUrlError::SignatureMismatch)?;

        // 4. Check timestamp freshness
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        if now - timestamp > MAX_SIGNATURE_AGE {
            return Err(VerifiedUrlError::TimestampTooOld);
        }

        // 5. Verify the URL origin is in allowed_origins
        let parsed_url =
            url::Url::parse(signed_url).map_err(|_| VerifiedUrlError::InvalidHeader)?;
        let origin = parsed_url.origin().unicode_serialization();

        if !merchant
            .allowed_origins
            .iter()
            .any(|allowed| allowed == &origin)
        {
            drop(merchant);
            return Err(VerifiedUrlError::OriginNotAllowed);
        }

        drop(merchant);
        Ok(VerifiedUrl)
    }
}
