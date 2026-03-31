//! Supported blockchains and stablecoins with their on-chain contract addresses.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
/// All blockchains supported by Ocrch
pub enum Blockchain {
    /// Ethereum mainnet.
    #[serde(rename = "eth")]
    Ethereum,
    /// Polygon (MATIC) mainnet.
    #[serde(rename = "polygon")]
    Polygon,
    /// Base mainnet (Coinbase L2).
    #[serde(rename = "base")]
    Base,
    /// Arbitrum One mainnet.
    #[serde(rename = "arb")]
    ArbitrumOne,
    /// Linea mainnet (ConsenSys L2).
    #[serde(rename = "linea")]
    Linea,
    /// Optimism mainnet.
    #[serde(rename = "op")]
    Optimism,
    /// Avalanche C-Chain.
    #[serde(rename = "avaxc")]
    AvalancheC,
    /// Tron mainnet.
    #[serde(rename = "tron")]
    Tron,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
/// All stablecoins supported by Ocrch
#[serde(rename_all = "UPPERCASE")]
pub enum Stablecoin {
    /// USD Coin (USDC).
    Usdc,
    /// Tether (USDT).
    Usdt,
    /// Dai (DAI).
    Dai,
}

impl Stablecoin {
    /// Return the static [`StablecoinData`] record for this coin.
    pub fn get_data(&self) -> StablecoinData {
        match self {
            Stablecoin::Usdc => USDC,
            Stablecoin::Usdt => USDT,
            Stablecoin::Dai => DAI,
        }
    }
}

/// Static metadata for a supported stablecoin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct StablecoinData {
    /// The stablecoin identifier.
    pub name: Stablecoin,
    /// Contract addresses on each supported blockchain, as `(chain, address)` pairs.
    pub contract_addresses: &'static [(Blockchain, &'static str)],
}

impl StablecoinData {
    /// Look up the contract address for `on_chain`.
    ///
    /// Returns `None` if this stablecoin is not deployed on the requested chain.
    pub fn get_contract_address(&self, on_chain: Blockchain) -> Option<&'static str> {
        self.contract_addresses
            .iter()
            .find(|(chain, _)| chain == &on_chain)
            .map(|(_, addr)| *addr)
    }
}

/// Static USDT contract-address table.
pub const USDT: StablecoinData = StablecoinData {
    name: Stablecoin::Usdt,
    contract_addresses: &[
        (
            Blockchain::Ethereum,
            "0xdAC17F958D2ee523a2206206994597C13D831ec7",
        ),
        (Blockchain::Tron, "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t"),
        (
            Blockchain::Polygon,
            "0x9702230A8Ea53601f5cD2dc00fDBc13d4dF4A8c7",
        ),
    ],
};

/// Static USDC contract-address table.
pub const USDC: StablecoinData = StablecoinData {
    name: Stablecoin::Usdc,
    contract_addresses: &[
        (
            Blockchain::Ethereum,
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
        ),
        (
            Blockchain::AvalancheC,
            "0xB97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E",
        ),
        (
            Blockchain::ArbitrumOne,
            "0xaf88d065e77c8cC2239327C5EDb3A432268e5831",
        ),
        (
            Blockchain::Polygon,
            "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359",
        ),
        (
            Blockchain::Optimism,
            "0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85",
        ),
        (
            Blockchain::Base,
            "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
        ),
        (
            Blockchain::Linea,
            "0x176211869cA2b568f2A7D4EE941E073a821EE1ff",
        ),
    ],
};

/// Static DAI contract-address table.
pub const DAI: StablecoinData = StablecoinData {
    name: Stablecoin::Dai,
    contract_addresses: &[
        (
            Blockchain::Ethereum,
            "0x6B175474E89094C44Da98b954EedeAC495271d0F",
        ),
        (
            Blockchain::AvalancheC,
            "0xbA7dEebBFC5fA1100Fb055a87773e1E99Cd3507a",
        ),
        (
            Blockchain::ArbitrumOne,
            "0xDA10009cBd5D07dd0CeCc66161FC93D7c9000da1",
        ),
        (
            Blockchain::Polygon,
            "0x82E64f49Ed5EC1bC6e43DAD4FC8Af9bb3A2312EE",
        ),
        (
            Blockchain::Optimism,
            "0xDA10009cBd5D07dd0CeCc66161FC93D7c9000da1",
        ),
        (
            Blockchain::Base,
            "0x50c5725949A6F0c72E6C4a641F24049A917DB0Cb",
        ),
        (
            Blockchain::Linea,
            "0x4AF15ec2A0BD43Db75dd04E62FAA3B8EF36b00d5",
        ),
    ],
};
