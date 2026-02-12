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
    pub blockchain: Blockchain,
    pub stablecoin: Stablecoin,
}

/// A single available chain-coin pair with its receiving wallet address.
///
/// Returned as part of the chain list so the frontend knows which
/// payment options are available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainCoinPair {
    pub blockchain: Blockchain,
    pub stablecoin: Stablecoin,
    pub wallet_address: String,
}

/// Response returned after a pending deposit is created.
///
/// Contains the wallet address the user should send funds to,
/// along with the expected amount and selected chain/coin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentDetail {
    pub order_id: Uuid,
    pub wallet_address: String,
    pub amount: rust_decimal::Decimal,
    pub blockchain: Blockchain,
    pub stablecoin: Stablecoin,
}
