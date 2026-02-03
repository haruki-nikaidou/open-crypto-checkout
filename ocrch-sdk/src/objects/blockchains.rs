use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
/// All blockchains supported by Ocrch
pub enum Blockchain {
    #[serde(rename = "eth")]
    Ethereum,
    #[serde(rename = "polygon")]
    Polygon,
    #[serde(rename = "base")]
    Base,
    #[serde(rename = "arb")]
    ArbitrumOne,
    #[serde(rename = "linea")]
    Linea,
    #[serde(rename = "op")]
    Optimism,
    #[serde(rename = "avaxc")]
    AvalancheC,
    #[serde(rename = "tron")]
    Tron,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
/// All stablecoins supported by Ocrch
#[serde(rename_all = "UPPERCASE")]
pub enum Stablecoin {
    Usdc,
    Usdt,
    Dai,
}
