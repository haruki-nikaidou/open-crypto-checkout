use axum::{Json, extract::Query, response::IntoResponse};
use kanau::processor::Processor;
use ocrch_core::entities::erc20_pending_deposit::{EtherScanChain, ListErc20PendingDeposits};
use ocrch_core::entities::trc20_pending_deposit::ListTrc20PendingDeposits;
use ocrch_core::entities::StablecoinName;
use ocrch_core::framework::DatabaseProcessor;
use ocrch_sdk::objects::admin::{AdminPendingDepositResponse, ListDepositsQuery, clamp_pagination};
use ocrch_sdk::objects::blockchains::Blockchain;

use crate::api::extractors::AdminAuth;
use crate::state::AppState;

use super::AdminApiError;

/// `GET /deposits` — list pending deposits with pagination and optional filters.
///
/// Queries both ERC-20 and TRC-20 tables, merges and sorts by `started_at` desc,
/// then applies limit/offset at the application level.
pub async fn list_deposits(
    state: axum::extract::State<AppState>,
    _auth: AdminAuth,
    Query(query): Query<ListDepositsQuery>,
) -> Result<impl IntoResponse, AdminApiError> {
    let processor = DatabaseProcessor {
        pool: state.db.clone(),
    };

    let (limit, offset) = clamp_pagination(query.limit, query.offset);

    let is_tron_only = query.blockchain == Some(Blockchain::Tron);
    let is_erc20_only = query.blockchain.is_some() && !is_tron_only;

    let fetch_limit = limit + offset;

    let mut results: Vec<AdminPendingDepositResponse> = Vec::new();

    if !is_tron_only {
        let erc20_chain = query.blockchain.and_then(|b| blockchain_to_etherscan(b).ok());

        let erc20 = processor
            .process(ListErc20PendingDeposits {
                limit: fetch_limit,
                offset: 0,
                order_id: query.order_id,
                chain: erc20_chain,
                token: query.token.map(|t| StablecoinName::from(t)),
            })
            .await
            .map_err(AdminApiError::Database)?;

        for d in &erc20 {
            results.push(AdminPendingDepositResponse {
                id: d.id,
                order_id: d.order,
                blockchain: d.chain.into(),
                token: d.token_name.into(),
                user_address: d.user_address.clone(),
                wallet_address: d.wallet_address.clone(),
                value: d.value,
                started_at: d.started_at.assume_utc().unix_timestamp(),
                last_scanned_at: d.last_scanned_at.assume_utc().unix_timestamp(),
            });
        }
    }

    if !is_erc20_only {
        let trc20 = processor
            .process(ListTrc20PendingDeposits {
                limit: fetch_limit,
                offset: 0,
                order_id: query.order_id,
                token: query.token.map(|t| StablecoinName::from(t)),
            })
            .await
            .map_err(AdminApiError::Database)?;

        for d in &trc20 {
            results.push(AdminPendingDepositResponse {
                id: d.id,
                order_id: d.order,
                blockchain: Blockchain::Tron,
                token: d.token_name.into(),
                user_address: d.user_address.clone(),
                wallet_address: d.wallet_address.clone(),
                value: d.value,
                started_at: d.started_at.assume_utc().unix_timestamp(),
                last_scanned_at: d.last_scanned_at.assume_utc().unix_timestamp(),
            });
        }
    }

    results.sort_by(|a, b| b.started_at.cmp(&a.started_at));

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
