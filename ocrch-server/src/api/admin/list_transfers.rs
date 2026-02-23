use axum::extract::Path;
use axum::{Json, extract::Query, response::IntoResponse};
use kanau::processor::Processor;
use ocrch_core::entities::erc20_pending_deposit::EtherScanChain;
use ocrch_core::entities::erc20_transfer::ListErc20TransfersByWallet;
use ocrch_core::entities::trc20_transfer::ListTrc20TransfersByWallet;
use ocrch_core::entities::{StablecoinName, TransferStatus};
use ocrch_core::framework::DatabaseProcessor;
use ocrch_sdk::objects::admin::{AdminTransferResponse, ListTransfersQuery, clamp_pagination};
use ocrch_sdk::objects::blockchains::Blockchain;

use crate::api::extractors::AdminAuth;
use crate::state::AppState;

use super::AdminApiError;

/// `GET /wallets/{address}/transfers` — list transfers for a wallet address.
///
/// Queries both ERC-20 and TRC-20 tables, merges and sorts by `created_at` desc,
/// then applies limit/offset at the application level.
pub async fn list_transfers(
    state: axum::extract::State<AppState>,
    _auth: AdminAuth,
    Path(address): Path<String>,
    Query(query): Query<ListTransfersQuery>,
) -> Result<impl IntoResponse, AdminApiError> {
    let processor = DatabaseProcessor {
        pool: state.db.clone(),
    };

    let (limit, offset) = clamp_pagination(query.limit, query.offset);

    let is_tron_only = query.blockchain == Some(Blockchain::Tron);
    let is_erc20_only = query.blockchain.is_some() && !is_tron_only;

    let fetch_limit = limit + offset;

    let mut results: Vec<AdminTransferResponse> = Vec::new();

    if !is_tron_only {
        let erc20_chain = query
            .blockchain
            .and_then(|b| blockchain_to_etherscan(b).ok());

        let erc20 = processor
            .process(ListErc20TransfersByWallet {
                wallet_address: address.clone(),
                limit: fetch_limit,
                offset: 0,
                status: query.status.map(|s| TransferStatus::from(s)),
                chain: erc20_chain,
                token: query.token.map(|t| StablecoinName::from(t)),
            })
            .await
            .map_err(AdminApiError::Database)?;

        for t in &erc20 {
            results.push(AdminTransferResponse {
                id: t.id,
                blockchain: t.chain.into(),
                token: t.token_name.into(),
                from_address: t.from_address.clone(),
                to_address: t.to_address.clone(),
                txn_hash: t.txn_hash.clone(),
                value: t.value,
                block_number: t.block_number,
                block_timestamp: t.block_timestamp,
                blockchain_confirmed: t.blockchain_confirmed,
                created_at: t.created_at.assume_utc().unix_timestamp(),
                status: t.status.into(),
                fulfillment_id: t.fulfillment_id,
            });
        }
    }

    if !is_erc20_only {
        let trc20 = processor
            .process(ListTrc20TransfersByWallet {
                wallet_address: address,
                limit: fetch_limit,
                offset: 0,
                status: query.status.map(|s| TransferStatus::from(s)),
                token: query.token.map(|t| StablecoinName::from(t)),
            })
            .await
            .map_err(AdminApiError::Database)?;

        for t in &trc20 {
            results.push(AdminTransferResponse {
                id: t.id,
                blockchain: Blockchain::Tron,
                token: t.token_name.into(),
                from_address: t.from_address.clone(),
                to_address: t.to_address.clone(),
                txn_hash: t.txn_hash.clone(),
                value: t.value,
                block_number: t.block_number,
                block_timestamp: t.block_timestamp,
                blockchain_confirmed: t.blockchain_confirmed,
                created_at: t.created_at.assume_utc().unix_timestamp(),
                status: t.status.into(),
                fulfillment_id: t.fulfillment_id,
            });
        }
    }

    results.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let page: Vec<_> = results
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();

    Ok(Json(page))
}

fn blockchain_to_etherscan(b: Blockchain) -> Result<EtherScanChain, ()> {
    match b {
        Blockchain::Ethereum => Ok(EtherScanChain::Ethereum),
        Blockchain::Polygon => Ok(EtherScanChain::Polygon),
        Blockchain::Base => Ok(EtherScanChain::Base),
        Blockchain::ArbitrumOne => Ok(EtherScanChain::ArbitrumOne),
        Blockchain::Linea => Ok(EtherScanChain::Linea),
        Blockchain::Optimism => Ok(EtherScanChain::Optimism),
        Blockchain::AvalancheC => Ok(EtherScanChain::AvalancheC),
        Blockchain::Tron => Err(()),
    }
}
