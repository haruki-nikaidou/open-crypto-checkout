//! Generic config store with change notification.
//!
//! `ConfigStore<T>` wraps `Arc<RwLock<T>>` and provides a watch-based
//! notification mechanism so that consumers can react to config changes
//! without polling.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{RwLock, RwLockReadGuard, watch};

/// A shared, versioned configuration store with change notification.
///
/// Wraps a value of type `T` behind `Arc<RwLock<T>>` and maintains an
/// incrementing version counter. Subscribers receive a [`ConfigWatcher`]
/// that can `await` the next change.
pub struct ConfigStore<T> {
    inner: Arc<ConfigStoreInner<T>>,
}

struct ConfigStoreInner<T> {
    data: RwLock<T>,
    version: AtomicU64,
    version_tx: watch::Sender<u64>,
}

/// Receives notifications when a [`ConfigStore`] is updated.
///
/// Call [`changed()`](ConfigWatcher::changed) to wait for the next update.
pub struct ConfigWatcher {
    version_rx: watch::Receiver<u64>,
}

// -- ConfigStore --------------------------------------------------------

impl<T> ConfigStore<T> {
    /// Create a new `ConfigStore` with the given initial value.
    pub fn new(initial: T) -> Self {
        let (version_tx, _) = watch::channel(0u64);
        Self {
            inner: Arc::new(ConfigStoreInner {
                data: RwLock::new(initial),
                version: AtomicU64::new(0),
                version_tx,
            }),
        }
    }

    /// Replace the stored value and notify all watchers.
    pub async fn update(&self, value: T) {
        let mut guard = self.inner.data.write().await;
        *guard = value;
        let new_version = self.inner.version.fetch_add(1, Ordering::Relaxed) + 1;
        // Drop the write guard before notifying so subscribers can
        // immediately acquire a read lock.
        drop(guard);
        let _ = self.inner.version_tx.send(new_version);
    }

    /// Read the current value.
    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        self.inner.data.read().await
    }

    /// Subscribe to change notifications.
    pub fn subscribe(&self) -> ConfigWatcher {
        ConfigWatcher {
            version_rx: self.inner.version_tx.subscribe(),
        }
    }
}

impl<T> Clone for ConfigStore<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

// -- ConfigWatcher ------------------------------------------------------

impl ConfigWatcher {
    /// Wait until the config store is updated.
    ///
    /// Returns `Ok(())` when a new version is available, or `Err` if the
    /// [`ConfigStore`] has been dropped.
    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        self.version_rx.changed().await
    }
}
