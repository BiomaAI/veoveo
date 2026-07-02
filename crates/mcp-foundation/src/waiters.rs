use std::collections::HashMap;

use tokio::sync::{Mutex, oneshot};

/// Waiters keyed by provider prediction/job id.
///
/// The webhook handler resolves a waiter when a provider callback arrives; the
/// task runner can fall back to provider polling if this receiver never fires.
#[derive(Default)]
pub struct WebhookWaiters<T> {
    pending: Mutex<HashMap<String, oneshot::Sender<T>>>,
}

impl<T> WebhookWaiters<T> {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }

    pub async fn register(&self, id: impl Into<String>) -> oneshot::Receiver<T> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.into(), tx);
        rx
    }

    pub async fn resolve(&self, id: &str, value: T) -> bool {
        if let Some(tx) = self.pending.lock().await.remove(id) {
            let _ = tx.send(value);
            return true;
        }
        false
    }

    pub async fn remove(&self, id: &str) {
        self.pending.lock().await.remove(id);
    }
}
