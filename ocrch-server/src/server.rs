//! Axum server setup and router configuration.

use crate::shutdown::shutdown_signal;
use crate::state::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Build the main application router.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // Health check endpoint
        .route("/health", get(health_check))
        // Ready check (includes database connectivity)
        .route("/ready", get(ready_check))
        // Add state to all routes
        .with_state(state)
}

/// Health check response.
#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

/// Simple health check - returns OK if the server is running.
async fn health_check() -> impl IntoResponse {
    Json(HealthResponse {
        status: "healthy",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Ready check response.
#[derive(Serialize)]
struct ReadyResponse {
    status: &'static str,
    database: &'static str,
}

/// Ready check - verifies database connectivity.
async fn ready_check(State(state): State<AppState>) -> impl IntoResponse {
    // Check database connectivity
    let db_status = match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => "connected",
        Err(_) => "disconnected",
    };

    let status = if db_status == "connected" {
        "ready"
    } else {
        "not_ready"
    };

    let status_code = if status == "ready" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(ReadyResponse {
            status,
            database: db_status,
        }),
    )
}

/// Run the server with graceful shutdown support.
pub async fn run_server(router: Router, addr: SocketAddr) -> Result<(), std::io::Error> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Server listening on {}", addr);

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
}
