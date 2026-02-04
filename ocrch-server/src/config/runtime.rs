//! Runtime configuration re-exports and utilities.
//!
//! The actual config types are defined in `ocrch-sdk::config`.
//! This module re-exports them for convenience.

pub use ocrch_sdk::config::{
    AdminConfig, MerchantConfig, ServerConfig, SharedConfig, WalletConfig,
};
