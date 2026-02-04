//! Application state shared across all request handlers.

use ocrch_core::config::SharedConfig;
use sqlx::PgPool;

/// Application state that is shared across all request handlers.
///
/// This is cloneable and cheap to pass around (everything is behind Arc).
#[derive(Clone)]
pub struct AppState {
    /// Database connection pool.
    pub db: PgPool,
    /// Shared configuration with separate locks for each section.
    pub config: SharedConfig,
}

impl AppState {
    /// Create a new AppState with the given database pool and configuration.
    pub fn new(db: PgPool, config: SharedConfig) -> Self {
        Self { db, config }
    }
}
