//! Custom Axum extractors for request processing.
//!
//! Provides `SignedBody<T>` which reads the `Ocrch-Signature` header,
//! extracts the raw JSON body, reconstructs a `SignedObject<T>`,
//! and verifies the HMAC-SHA256 signature against the merchant secret.

use axum::{
    extract::{FromRequest, Request},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use ocrch_sdk::objects::{Signature, SignedObject};

use crate::state::AppState;

/// Header name for the HMAC signature.
const SIGNATURE_HEADER: &str = "Ocrch-Signature";

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
    BodyReadError(String),
    /// Failed to deserialize JSON body.
    JsonError(String),
    /// HMAC verification failed or timestamp too old.
    VerificationFailed(String),
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
            SignedBodyError::BodyReadError(_) => {
                (StatusCode::BAD_REQUEST, "failed to read request body")
            }
            SignedBodyError::JsonError(_) => (StatusCode::BAD_REQUEST, "invalid JSON body"),
            SignedBodyError::VerificationFailed(_) => {
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
            .map_err(|e| SignedBodyError::BodyReadError(e.to_string()))?;

        let json = String::from_utf8(body_bytes.to_vec())
            .map_err(|e| SignedBodyError::BodyReadError(e.to_string()))?;

        // 3. Deserialize the body to get the typed value
        let body: T =
            serde_json::from_str(&json).map_err(|e| SignedBodyError::JsonError(e.to_string()))?;

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
            .map_err(|e| SignedBodyError::VerificationFailed(e.to_string()))?;

        drop(merchant);

        Ok(SignedBody(verified_body))
    }
}
