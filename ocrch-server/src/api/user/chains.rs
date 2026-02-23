use axum::{Json, extract::State, response::IntoResponse};
use ocrch_sdk::objects::ChainCoinPair;

use super::UserApiError;
use crate::api::extractors::VerifiedUrl;
use crate::state::AppState;

/// `GET /chains` — list available blockchain + stablecoin payment options.
///
/// Returns every (blockchain, stablecoin, wallet_address) triple derived
/// from the configured wallets.
pub(super) async fn get_chains(
    state: State<AppState>,
    _verified: VerifiedUrl,
) -> Result<impl IntoResponse, UserApiError> {
    let wallets = state.config.wallets.read().await;
    let pairs: Vec<ChainCoinPair> = wallets
        .iter()
        .flat_map(|w| {
            w.enabled_coins.iter().map(move |coin| ChainCoinPair {
                blockchain: w.blockchain,
                stablecoin: *coin,
                wallet_address: w.address.clone(),
            })
        })
        .collect();
    drop(wallets);
    Ok(Json(pairs))
}
