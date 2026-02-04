//! Event system for the event-driven architecture.
//!
//! This module provides event types and channel infrastructure for
//! the asynchronous, event-driven processing pipeline.
//!
//! # Event Flow
//!
//! 1. `PendingDepositChanged` -> `PoolingManager`
//! 2. `PoolingManager` emits `PoolingTick` -> `BlockchainSync`
//! 3. `BlockchainSync` emits `MatchTick` -> `OrderBookWatcher`
//! 4. `OrderBookWatcher` emits `WebhookEvent` -> `WebhookSender`
//!
//! All events are idempotent and ephemeral - they carry identifiers
//! rather than full data, and processors re-fetch from DB.

pub mod channels;
pub mod types;

pub use channels::{
    match_tick_channel, pending_deposit_changed_channel, pooling_tick_channel,
    webhook_event_channel, EventSenders, MatchTickReceiver, MatchTickSender,
    PendingDepositChangedReceiver, PendingDepositChangedSender, PoolingTickReceiver,
    PoolingTickSender, WebhookEventReceiver, WebhookEventSender, DEFAULT_CHANNEL_BUFFER,
};

pub use types::{BlockchainTarget, MatchTick, PendingDepositChanged, PoolingTick, WebhookEvent};
