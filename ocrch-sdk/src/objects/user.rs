//! User API request and response types.
//!
//! These types are used by the checkout frontend to interact with the
//! headless backend on behalf of the paying user.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::blockchains::{Blockchain, Stablecoin};

/// Request body for selecting a blockchain + stablecoin payment method.
///
/// Sent by the checkout frontend when the user picks a chain and coin.
/// This triggers creation of a new pending deposit for the order.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SelectPaymentMethod {
    /// The blockchain to pay on.
    pub blockchain: Blockchain,
    /// The stablecoin to pay with.
    pub stablecoin: Stablecoin,
}

/// A single available chain-coin pair with its receiving wallet address.
///
/// Returned as part of the chain list so the frontend knows which
/// payment options are available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainCoinPair {
    /// Blockchain for this payment option.
    pub blockchain: Blockchain,
    /// Stablecoin for this payment option.
    pub stablecoin: Stablecoin,
    /// Wallet address the user should send funds to.
    pub wallet_address: String,
}

/// Response returned after a pending deposit is created.
///
/// Contains the wallet address the user should send funds to,
/// along with the expected amount and selected chain/coin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentDetail {
    /// Internal order ID.
    pub order_id: Uuid,
    /// Wallet address the user must send funds to.
    pub wallet_address: String,
    /// Exact amount the user must send.
    pub amount: rust_decimal::Decimal,
    /// Blockchain on which the payment is expected.
    pub blockchain: Blockchain,
    /// Stablecoin the payment must be made in.
    pub stablecoin: Stablecoin,
}
