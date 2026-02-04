//! BlockchainSync processor.
//!
//! The BlockchainSync is responsible for:
//! - Receiving `PoolingTick` events
//! - Fetching new transfers from blockchain explorer APIs
//! - Writing transfers to the database (with conflict handling for idempotency)
//! - Emitting `MatchTick` events after syncing
//!
//! Each enabled token on each blockchain has its own BlockchainSync instance.

use crate::entities::StablecoinName;
use crate::entities::erc20_pending_deposit::EtherScanChain;
use crate::entities::erc20_transfer::{Erc20TokenTransfer, Erc20TransferInsert};
use crate::entities::trc20_transfer::{Trc20TokenTransfer, Trc20TransferInsert};
use crate::events::{BlockchainTarget, MatchTick, MatchTickSender, PoolingTickReceiver};
use async_trait::async_trait;
use sqlx::PgPool;
use thiserror::Error;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

/// Errors that can occur during blockchain sync.
#[derive(Debug, Error)]
pub enum SyncError {
    /// Database error
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// API request error
    #[error("API request error: {0}")]
    Request(#[from] reqwest::Error),

    /// API response parsing error
    #[error("API response parsing error: {0}")]
    Parse(String),

    /// Rate limit exceeded
    #[error("rate limit exceeded, retry after {retry_after_secs} seconds")]
    RateLimited { retry_after_secs: u64 },

    /// API returned an error
    #[error("API error: {message}")]
    ApiError { message: String },

    /// Token not supported
    #[error("token not supported")]
    UnsupportedToken
}

/// Trait for blockchain sync implementations.
///
/// Each blockchain type (ERC-20 chains, TRC-20) implements this trait
/// to handle its specific API and data format.
#[async_trait]
pub trait BlockchainSync: Send + Sync {
    /// Sync new transfers from the blockchain explorer API.
    ///
    /// Returns the number of new transfers synced.
    async fn sync(&self, pool: &PgPool) -> Result<u32, SyncError>;

    /// Get the blockchain target for this sync.
    fn blockchain_target(&self) -> BlockchainTarget;

    /// Get the token this sync handles.
    fn token(&self) -> StablecoinName;
}

/// ERC-20 blockchain sync implementation.
///
/// Handles syncing from EtherScan-compatible APIs for various EVM chains.
pub struct Erc20BlockchainSync {
    chain: EtherScanChain,
    token: StablecoinName,
    wallet_address: String,
    api_key: String,
    http_client: reqwest::Client,
}

impl Erc20BlockchainSync {
    const ETHERSCAN_API_URL: &str = "https://api.etherscan.io/v2/api";

    /// Fetch transfers from the EtherScan API.
    async fn fetch_transfers(&self, start_block: i64) -> Result<Vec<Erc20TokenTransferResponseItem>, SyncError> {
        #[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
        struct EtherScanResponse<T> {
            status: String,
            message: String,
            result: T,
        }

        let Some(contract_address) = self.token.get_data().get_contract_address(self.chain.into()) else {
            return Err(SyncError::UnsupportedToken)
        };
        let chain_id = self.chain as i32;
        let response = self
            .http_client
            .get(Self::ETHERSCAN_API_URL)
            .query(&[
                ("apiKey", self.api_key.as_str()),
                ("chainid", chain_id.to_string().as_str()),
                ("module", "account"),
                ("action", "tokentx"),
                ("contractaddress", contract_address),
                ("address", self.wallet_address.as_str()),
                ("startblock", start_block.to_string().as_str()),
                ("page", "1"),
                ("offset", "100"),
                ("sort", "asc"),
            ])
            .send()
            .await?;
        let response: EtherScanResponse<Vec<Erc20TokenTransferResponseItem>> =
            response.json().await?;
        if response.status != "1" {
            return Err(SyncError::ApiError {
                message: response.message,
            });
        }
        Ok(response.result)
    }

    /// Get the last synced block number from the database.
    async fn get_last_block(&self, pool: &PgPool) -> Result<i64, SyncError> {
        let result = Erc20TokenTransfer::get_last_block(pool, self.chain, self.token).await?;
        Ok(result)
    }

    /// Insert transfers into the database.
    ///
    /// Uses ON CONFLICT DO NOTHING to ensure idempotency.
    async fn insert_transfers(
        &self,
        pool: &PgPool,
        transfers: Vec<Erc20TransferData>,
    ) -> Result<u32, SyncError> {
        if transfers.is_empty() {
            return Ok(0);
        }

        let mut inserted = 0u32;

        for transfer in transfers {
            // Only process incoming transfers to our wallet
            if transfer.to.to_lowercase() != self.wallet_address.to_lowercase() {
                continue;
            }

            let block_number: i64 = transfer
                .block_number
                .parse()
                .map_err(|e| SyncError::Parse(format!("Invalid block number: {}", e)))?;

            let block_timestamp: i64 = transfer
                .time_stamp
                .parse()
                .map_err(|e| SyncError::Parse(format!("Invalid timestamp: {}", e)))?;

            let value: rust_decimal::Decimal = transfer
                .value
                .parse()
                .map_err(|e| SyncError::Parse(format!("Invalid value: {}", e)))?;

            // Adjust for token decimals (most stablecoins use 6 or 18 decimals)
            let decimals: u32 = transfer
                .token_decimal
                .parse()
                .map_err(|e| SyncError::Parse(format!("Invalid decimals: {}", e)))?;

            let divisor = rust_decimal::Decimal::from(10u64.pow(decimals));
            let normalized_value = value / divisor;

            let transfer_insert = Erc20TransferInsert {
                token_name: self.token,
                chain: self.chain,
                from_address: transfer.from,
                to_address: transfer.to,
                txn_hash: transfer.hash,
                value: normalized_value,
                block_number,
                block_timestamp,
            };

            if Erc20TokenTransfer::insert(pool, &transfer_insert).await? {
                inserted += 1;
            }
        }

        Ok(inserted)
    }
}

#[async_trait]
impl BlockchainSync for Erc20BlockchainSync {
    async fn sync(&self, pool: &PgPool) -> Result<u32, SyncError> {
        let last_block = self.get_last_block(pool).await?;
        let start_block = if last_block > 0 { last_block } else { 0 };

        debug!(
            chain = ?self.chain,
            token = ?self.token,
            start_block = start_block,
            "Fetching ERC-20 transfers"
        );

        let transfers = self.fetch_transfers(start_block).await?;

        let inserted = self.insert_transfers(pool, transfers).await?;

        debug!(
            chain = ?self.chain,
            token = ?self.token,
            inserted = inserted,
            "Synced ERC-20 transfers"
        );

        Ok(inserted)
    }

    fn blockchain_target(&self) -> BlockchainTarget {
        BlockchainTarget::Erc20(self.chain)
    }

    fn token(&self) -> StablecoinName {
        self.token
    }
}

/// TRC-20 blockchain sync implementation.
///
/// Handles syncing from TronScan API for the Tron network.
pub struct Trc20BlockchainSync {
    token: StablecoinName,
    wallet_address: String,
    contract_address: String,
    http_client: reqwest::Client,
}

impl Trc20BlockchainSync {
    /// Create a new Trc20BlockchainSync.
    ///
    /// # Arguments
    ///
    /// * `token` - The stablecoin to track
    /// * `wallet_address` - The wallet address to monitor for incoming transfers
    /// * `contract_address` - The token contract address
    pub fn new(token: StablecoinName, wallet_address: String, contract_address: String) -> Self {
        Self {
            token,
            wallet_address,
            contract_address,
            http_client: reqwest::Client::new(),
        }
    }

    /// Fetch transfers from the TronScan API.
    async fn fetch_transfers(
        &self,
        start_timestamp: i64,
    ) -> Result<Vec<Trc20TransferData>, SyncError> {
        let url = format!(
            "https://apilist.tronscanapi.com/api/filter/trc20/transfers?limit=200&contract_address={}&toAddress={}&start_timestamp={}",
            self.contract_address, self.wallet_address, start_timestamp
        );

        let response = self.http_client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SyncError::RateLimited {
                retry_after_secs: 5,
            });
        }

        let response_json: TronScanResponse = response.json().await?;

        Ok(response_json.token_transfers)
    }

    /// Get the last synced timestamp from the database.
    async fn get_last_timestamp(&self, pool: &PgPool) -> Result<i64, SyncError> {
        let result = Trc20TokenTransfer::get_last_timestamp(pool, self.token).await?;
        Ok(result)
    }

    /// Insert transfers into the database.
    ///
    /// Uses ON CONFLICT DO NOTHING to ensure idempotency.
    async fn insert_transfers(
        &self,
        pool: &PgPool,
        transfers: Vec<Trc20TransferData>,
    ) -> Result<u32, SyncError> {
        if transfers.is_empty() {
            return Ok(0);
        }

        let mut inserted = 0u32;

        for transfer in transfers {
            // Only process incoming transfers to our wallet
            if transfer.to_address.to_lowercase() != self.wallet_address.to_lowercase() {
                continue;
            }

            let value: rust_decimal::Decimal = transfer
                .quant
                .parse()
                .map_err(|e| SyncError::Parse(format!("Invalid value: {}", e)))?;

            // TRC-20 USDT/USDC typically use 6 decimals
            let divisor = rust_decimal::Decimal::from(10u64.pow(transfer.decimals as u32));
            let normalized_value = value / divisor;

            let transfer_insert = Trc20TransferInsert {
                token_name: self.token,
                from_address: transfer.from_address,
                to_address: transfer.to_address,
                txn_hash: transfer.transaction_id,
                value: normalized_value,
                block_number: transfer.block,
                block_timestamp: transfer.block_ts,
            };

            if Trc20TokenTransfer::insert(pool, &transfer_insert).await? {
                inserted += 1;
            }
        }

        Ok(inserted)
    }
}

#[async_trait]
impl BlockchainSync for Trc20BlockchainSync {
    async fn sync(&self, pool: &PgPool) -> Result<u32, SyncError> {
        let last_timestamp = self.get_last_timestamp(pool).await?;
        let start_timestamp = if last_timestamp > 0 {
            last_timestamp
        } else {
            0
        };

        debug!(
            token = ?self.token,
            start_timestamp = start_timestamp,
            "Fetching TRC-20 transfers"
        );

        let transfers = self.fetch_transfers(start_timestamp).await?;

        let inserted = self.insert_transfers(pool, transfers).await?;

        debug!(
            token = ?self.token,
            inserted = inserted,
            "Synced TRC-20 transfers"
        );

        Ok(inserted)
    }

    fn blockchain_target(&self) -> BlockchainTarget {
        BlockchainTarget::Trc20
    }

    fn token(&self) -> StablecoinName {
        self.token
    }
}

/// Runner for a BlockchainSync instance.
///
/// This wraps a BlockchainSync implementation and handles:
/// - Receiving PoolingTick events
/// - Calling sync()
/// - Emitting MatchTick events
pub struct BlockchainSyncRunner<S: BlockchainSync> {
    sync: S,
    pool: PgPool,
    tick_rx: PoolingTickReceiver,
    match_tx: MatchTickSender,
    shutdown_rx: watch::Receiver<bool>,
}

impl<S: BlockchainSync + 'static> BlockchainSyncRunner<S> {
    /// Create a new BlockchainSyncRunner.
    pub fn new(
        sync: S,
        pool: PgPool,
        tick_rx: PoolingTickReceiver,
        match_tx: MatchTickSender,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        Self {
            sync,
            pool,
            tick_rx,
            match_tx,
            shutdown_rx,
        }
    }

    /// Run the BlockchainSyncRunner.
    pub async fn run(mut self) {
        let blockchain = self.sync.blockchain_target();
        let token = self.sync.token();

        info!(
            blockchain = %blockchain,
            token = ?token,
            "BlockchainSyncRunner started"
        );

        loop {
            tokio::select! {
                biased;

                // Check for shutdown
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        info!(
                            blockchain = %blockchain,
                            token = ?token,
                            "BlockchainSyncRunner shutting down"
                        );
                        break;
                    }
                }

                // Receive PoolingTick events
                Some(tick) = self.tick_rx.recv() => {
                    // Verify this tick is for us
                    if tick.blockchain != blockchain || tick.token != token {
                        warn!(
                            expected_blockchain = %blockchain,
                            expected_token = ?token,
                            received_blockchain = %tick.blockchain,
                            received_token = ?tick.token,
                            "Received mismatched PoolingTick"
                        );
                        continue;
                    }

                    // Perform sync
                    match self.sync.sync(&self.pool).await {
                        Ok(transfers_synced) => {
                            debug!(
                                blockchain = %blockchain,
                                token = ?token,
                                transfers_synced = transfers_synced,
                                "Sync completed"
                            );

                            // Emit MatchTick
                            let match_tick = MatchTick {
                                blockchain,
                                token,
                                transfers_synced,
                            };

                            if let Err(e) = self.match_tx.send(match_tick).await {
                                error!(
                                    blockchain = %blockchain,
                                    token = ?token,
                                    error = %e,
                                    "Failed to send MatchTick"
                                );
                            }
                        }
                        Err(e) => {
                            error!(
                                blockchain = %blockchain,
                                token = ?token,
                                error = %e,
                                "Sync failed"
                            );

                            // Still emit MatchTick with 0 transfers so matching can proceed
                            // for any previously synced transfers
                            let match_tick = MatchTick {
                                blockchain,
                                token,
                                transfers_synced: 0,
                            };

                            let _ = self.match_tx.send(match_tick).await;
                        }
                    }
                }

                else => {
                    info!(
                        blockchain = %blockchain,
                        token = ?token,
                        "PoolingTick channel closed"
                    );
                    break;
                }
            }
        }

        info!(
            blockchain = %blockchain,
            token = ?token,
            "BlockchainSyncRunner shutdown complete"
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Erc20TokenTransferResponseItem {
    pub block_number: String,
    pub time_stamp: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub token_decimal: String,
}

// API response types for TronScan
#[derive(Debug, serde::Deserialize)]
struct TronScanResponse {
    #[serde(default)]
    token_transfers: Vec<Trc20TransferData>,
}

#[derive(Debug, serde::Deserialize)]
struct Trc20TransferData {
    transaction_id: String,
    block_ts: i64,
    block: i64,
    from_address: String,
    to_address: String,
    quant: String,
    #[serde(default = "default_decimals")]
    decimals: i32,
}

fn default_decimals() -> i32 {
    6 // Most TRC-20 stablecoins use 6 decimals
}
