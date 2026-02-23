use axum::{Json, response::IntoResponse};
use ocrch_sdk::objects::admin::AdminWalletResponse;

use crate::api::extractors::AdminAuth;
use crate::state::AppState;

/// `GET /wallets` — show wallets and enabled coins from config.
pub async fn show_wallets(
    state: axum::extract::State<AppState>,
    _auth: AdminAuth,
) -> impl IntoResponse {
    let wallets = state.config.wallets.read().await;

    let response: Vec<AdminWalletResponse> = wallets
        .iter()
        .map(|w| AdminWalletResponse {
            blockchain: w.blockchain,
            address: w.address.clone(),
            enabled_coins: w.enabled_coins.clone(),
        })
        .collect();

    Json(response)
}
