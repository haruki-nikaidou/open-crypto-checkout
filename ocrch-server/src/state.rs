//! Application state shared across all request handlers.

use ocrch_core::config::SharedConfig;
use ocrch_core::events::EventSenders;
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
    /// Event channel senders for emitting events from API handlers.
    pub event_senders: EventSenders,
}

impl AppState {
    /// Create a new AppState with the given database pool, configuration, and event senders.
    pub fn new(db: PgPool, config: SharedConfig, event_senders: EventSenders) -> Self {
        Self {
            db,
            config,
            event_senders,
        }
    }
}
