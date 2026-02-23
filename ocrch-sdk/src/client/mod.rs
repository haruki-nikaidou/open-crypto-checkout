//! HTTP clients for all Ocrch APIs.
//!
//! Gated behind the `client` cargo feature so downstream crates that only
//! need the shared types do not pull in `reqwest`.

mod admin;
mod service;
mod user;
mod webhook;

pub use admin::AdminClient;
pub use service::ServiceClient;
pub use user::UserClient;
pub use webhook::verify_webhook;

use reqwest::StatusCode;

use crate::signature::SignatureError;

/// Errors produced by the SDK HTTP clients.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// Transport-level failure (DNS, TLS, connection reset, …).
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    /// HMAC signature could not be computed or verified.
    #[error("signature error: {0}")]
    Signature(#[from] SignatureError),

    /// The server returned a non-2xx status code.
    #[error("api error: status {status}, body: {body}")]
    Api { status: StatusCode, body: String },

    /// Response body could not be deserialized.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// The base URL could not be joined with the endpoint path.
    #[error("invalid url: {0}")]
    Url(#[from] url::ParseError),
}
