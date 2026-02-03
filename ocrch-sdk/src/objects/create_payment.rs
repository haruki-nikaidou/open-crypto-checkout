use crate::objects::blockchains;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaymentCreatingEssential {
    pub amount: rust_decimal::Decimal,
    pub expecting_wallet_address: Option<String>,
    pub order_id: String,
    pub blockchain: blockchains::Blockchain,
    pub stablecoin: blockchains::Stablecoin,
    pub webhook_url: String,
}
