//! Event channel factories and handles.
//!
//! Provides factory functions for creating event channels with appropriate
//! buffer sizes for the event-driven architecture.

use super::types::{MatchTick, PendingDepositChanged, PoolingTick, WebhookEvent};
use tokio::sync::mpsc;

/// Default buffer size for event channels.
///
/// This provides enough buffer to handle bursts while keeping memory bounded.
pub const DEFAULT_CHANNEL_BUFFER: usize = 256;

/// Sender handle for PendingDepositChanged events.
pub type PendingDepositChangedSender = mpsc::Sender<PendingDepositChanged>;
/// Receiver handle for PendingDepositChanged events.
pub type PendingDepositChangedReceiver = mpsc::Receiver<PendingDepositChanged>;

/// Sender handle for PoolingTick events.
pub type PoolingTickSender = mpsc::Sender<PoolingTick>;
/// Receiver handle for PoolingTick events.
pub type PoolingTickReceiver = mpsc::Receiver<PoolingTick>;

/// Sender handle for MatchTick events.
pub type MatchTickSender = mpsc::Sender<MatchTick>;
/// Receiver handle for MatchTick events.
pub type MatchTickReceiver = mpsc::Receiver<MatchTick>;

/// Sender handle for WebhookEvent events.
pub type WebhookEventSender = mpsc::Sender<WebhookEvent>;
/// Receiver handle for WebhookEvent events.
pub type WebhookEventReceiver = mpsc::Receiver<WebhookEvent>;

/// Create a new PendingDepositChanged channel.
///
/// Returns a (sender, receiver) pair for PendingDepositChanged events.
/// Multiple senders can be cloned from the returned sender.
pub fn pending_deposit_changed_channel() -> (PendingDepositChangedSender, PendingDepositChangedReceiver)
{
    mpsc::channel(DEFAULT_CHANNEL_BUFFER)
}

/// Create a new PoolingTick channel.
///
/// Returns a (sender, receiver) pair for PoolingTick events.
/// Each BlockchainSync instance should have its own channel.
pub fn pooling_tick_channel() -> (PoolingTickSender, PoolingTickReceiver) {
    mpsc::channel(DEFAULT_CHANNEL_BUFFER)
}

/// Create a new MatchTick channel.
///
/// Returns a (sender, receiver) pair for MatchTick events.
pub fn match_tick_channel() -> (MatchTickSender, MatchTickReceiver) {
    mpsc::channel(DEFAULT_CHANNEL_BUFFER)
}

/// Create a new WebhookEvent channel.
///
/// Returns a (sender, receiver) pair for WebhookEvent events.
pub fn webhook_event_channel() -> (WebhookEventSender, WebhookEventReceiver) {
    mpsc::channel(DEFAULT_CHANNEL_BUFFER)
}

/// Container for all event channel senders.
///
/// This provides a convenient way to pass around all event senders
/// to components that need to emit events.
#[derive(Clone)]
pub struct EventSenders {
    /// Sender for PendingDepositChanged events
    pub pending_deposit_changed: PendingDepositChangedSender,
    /// Sender for MatchTick events (used by BlockchainSync)
    pub match_tick: MatchTickSender,
    /// Sender for WebhookEvent events
    pub webhook_event: WebhookEventSender,
}

impl EventSenders {
    /// Create a new EventSenders container.
    pub fn new(
        pending_deposit_changed: PendingDepositChangedSender,
        match_tick: MatchTickSender,
        webhook_event: WebhookEventSender,
    ) -> Self {
        Self {
            pending_deposit_changed,
            match_tick,
            webhook_event,
        }
    }
}
