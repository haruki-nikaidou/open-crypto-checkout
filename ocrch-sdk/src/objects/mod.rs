pub mod blockchains;
pub mod create_payment;
pub mod webhook;

pub use blockchains::{Blockchain, Stablecoin};
pub use create_payment::{GetOrderRequest, OrderResponse, PaymentCreatingEssential};
pub use webhook::{OrderStatus, OrderStatusChangedPayload, TransferStatus, UnknownTransferPayload};

pub trait Signature: for<'de> serde::Deserialize<'de> + serde::Serialize {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedObject<T: Signature> {
    pub body: T,
    pub timestamp: i64,
    pub json: String,
    pub signature: Box<[u8]>,
}

impl<T: Signature> SignedObject<T> {
    const MAX_AGE: i64 = 5 * 60;
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
    pub fn verify(self, key: &[u8]) -> Result<T, SignedObjectParseError> {
        let data = format!("{}.{}", self.timestamp, self.json);
        ring::hmac::verify(
            &ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key),
            data.as_bytes(),
            self.signature.as_ref(),
        )?;
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        if now - self.timestamp > Self::MAX_AGE {
            return Err(SignedObjectParseError::TimestampTooOld);
        }
        Ok(self.body)
    }
    pub fn stringify_signature(&self) -> String {
        fast32::base64::RFC4648_NOPAD.encode(&self.signature)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SignedObjectParseError {
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid signature")]
    SignatureMismatch(#[from] ring::error::Unspecified),
    #[error("timestamp too old")]
    TimestampTooOld,
}
