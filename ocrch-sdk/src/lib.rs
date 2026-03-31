//! Ocrch SDK — shared types and HTTP clients for the Ocrch crypto-checkout APIs.
//!
//! # Crate layout
//!
//! - [`objects`] – wire types (request / response structs, enums) shared by
//!   all three APIs.
//! - [`signature`] – HMAC-SHA256 signing and verification primitives.
//! - [`client`] (feature `client`) – typed HTTP clients for the Admin, Service,
//!   and User APIs, plus a webhook verification helper.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Wire types shared across all Ocrch APIs.
pub mod objects;
pub mod signature;

#[cfg(feature = "client")]
pub mod client;
