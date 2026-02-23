//! Custom Axum extractors for request authentication.
//!
//! Provides:
//! - `SignedBody<T>` — verifies the `Ocrch-Signature` header against a signed JSON body
//!   (used by the Service API).
//! - `VerifiedUrl` — verifies the `Ocrch-Signature` header against a signed frontend URL
//!   carried in the `Ocrch-Signed-Url` header (used by the User API).
//!
//! All cryptographic operations are delegated to [`ocrch_sdk::signature`].

use axum::{
    extract::{FromRequest, FromRequestParts, Request},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use ocrch_sdk::signature::{
    self, SIGNATURE_HEADER, SIGNED_URL_HEADER, Signature, SignatureError, SignedObject,
};

use crate::state::AppState;

// ---------------------------------------------------------------------------
// SignedBody — Service API authentication via signed JSON body
// ---------------------------------------------------------------------------

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

/// Errors that can occur during signed-body verification.
#[derive(Debug, thiserror::Error)]
pub enum SignedBodyError {
    #[error("missing Ocrch-Signature header")]
    MissingHeader,
    #[error("invalid Ocrch-Signature header format")]
    InvalidHeader,
    #[error("invalid signature encoding")]
    InvalidBase64,
    #[error("failed to read request body")]
    BodyReadError,
    #[error("invalid JSON body: {0}")]
    JsonError(serde_json::Error),
    #[error("signature verification failed")]
    VerificationFailed,
}

impl From<SignatureError> for SignedBodyError {
    fn from(err: SignatureError) -> Self {
        match err {
            SignatureError::InvalidFormat => Self::InvalidHeader,
            SignatureError::InvalidBase64 => Self::InvalidBase64,
            SignatureError::Json(e) => Self::JsonError(e),
            SignatureError::SignatureMismatch | SignatureError::Expired => Self::VerificationFailed,
        }
    }
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

impl<T: Signature + Send> FromRequest<AppState> for SignedBody<T> {
    type Rejection = SignedBodyError;

    async fn from_request(req: Request, state: &AppState) -> Result<Self, Self::Rejection> {
        let header_value = req
            .headers()
            .get(SIGNATURE_HEADER)
            .ok_or(SignedBodyError::MissingHeader)?
            .to_str()
            .map_err(|_| SignedBodyError::InvalidHeader)?
            .to_owned();

        let body_bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024)
            .await
            .map_err(|_| SignedBodyError::BodyReadError)?;

        let json =
            String::from_utf8(body_bytes.to_vec()).map_err(|_| SignedBodyError::BodyReadError)?;

        let signed = SignedObject::<T>::from_header_and_body(&header_value, json)?;

        let merchant = state.config.merchant.read().await;
        let verified_body = signed.verify(merchant.secret_bytes())?;
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
    MissingSignature,
    MissingUrl,
    InvalidHeader,
    InvalidBase64,
    SignatureMismatch,
    TimestampTooOld,
    OriginNotAllowed,
}

impl From<SignatureError> for VerifiedUrlError {
    fn from(err: SignatureError) -> Self {
        match err {
            SignatureError::InvalidFormat => Self::InvalidHeader,
            SignatureError::InvalidBase64 => Self::InvalidBase64,
            SignatureError::Json(_) => Self::InvalidHeader,
            SignatureError::SignatureMismatch => Self::SignatureMismatch,
            SignatureError::Expired => Self::TimestampTooOld,
        }
    }
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
            VerifiedUrlError::InvalidHeader => (StatusCode::BAD_REQUEST, "invalid header format"),
            VerifiedUrlError::InvalidBase64 => {
                (StatusCode::BAD_REQUEST, "invalid signature encoding")
            }
            VerifiedUrlError::SignatureMismatch => {
                (StatusCode::UNAUTHORIZED, "signature verification failed")
            }
            VerifiedUrlError::TimestampTooOld => (StatusCode::UNAUTHORIZED, "signature expired"),
            VerifiedUrlError::OriginNotAllowed => (StatusCode::FORBIDDEN, "origin not allowed"),
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
        let sig_value = parts
            .headers
            .get(SIGNATURE_HEADER)
            .ok_or(VerifiedUrlError::MissingSignature)?
            .to_str()
            .map_err(|_| VerifiedUrlError::InvalidHeader)?;

        let (timestamp, signature_bytes) = signature::parse_signature_header(sig_value)?;

        let signed_url = parts
            .headers
            .get(SIGNED_URL_HEADER)
            .ok_or(VerifiedUrlError::MissingUrl)?
            .to_str()
            .map_err(|_| VerifiedUrlError::InvalidHeader)?;

        let merchant = state.config.merchant.read().await;
        signature::verify_url(
            signed_url,
            timestamp,
            &signature_bytes,
            merchant.secret_bytes(),
        )?;

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
