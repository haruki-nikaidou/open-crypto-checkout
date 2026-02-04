//! Application state shared across all request handlers.

use crate::config::runtime::RuntimeConfig;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Application state that is shared across all request handlers.
///
/// This is cloneable and cheap to pass around (everything is behind Arc).
#[derive(Clone)]
pub struct AppState {
    /// Database connection pool.
    pub db: PgPool,
    /// Runtime configuration (can be reloaded via SIGHUP).
    pub config: Arc<RwLock<RuntimeConfig>>,
}

impl AppState {
    /// Create a new AppState with the given database pool and configuration.
    pub fn new(db: PgPool, config: RuntimeConfig) -> Self {
        Self {
            db,
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// Get a read lock on the configuration.
    pub async fn config(&self) -> tokio::sync::RwLockReadGuard<'_, RuntimeConfig> {
        self.config.read().await
    }

    /// Update the configuration (used during SIGHUP reload).
    pub async fn update_config(&self, new_config: RuntimeConfig) {
        let mut config = self.config.write().await;
        *config = new_config;
    }
}
