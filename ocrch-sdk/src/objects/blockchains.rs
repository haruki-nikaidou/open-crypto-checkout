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

impl Stablecoin {
    pub fn get_data(&self) -> StablecoinData {
        match self {
            Stablecoin::Usdc => USDC,
            Stablecoin::Usdt => USDT,
            Stablecoin::Dai => DAI,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct StablecoinData {
    pub name: Stablecoin,
    pub contract_addresses: &'static [(Blockchain, &'static str)],
}

impl StablecoinData {
    pub fn get_contract_address(&self, on_chain: Blockchain) -> Option<&'static str> {
        self.contract_addresses
            .iter()
            .find(|(chain, _)| chain == &on_chain)
            .map(|(_, addr)| *addr)
    }
}

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
