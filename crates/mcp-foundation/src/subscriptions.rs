use std::collections::HashMap;

use rmcp::{RoleServer, model::ResourceUpdatedNotificationParam, service::Peer};
use tokio::sync::Mutex;

/// In-memory resource subscription registry.
///
/// Subscription identity is intentionally coarse for now: unsubscribe clears all
/// peers for a URI. Persistence and per-session accounting are layered later.
#[derive(Default)]
pub struct SubscriptionHub {
    subscribers: Mutex<HashMap<String, Vec<Peer<RoleServer>>>>,
}

impl SubscriptionHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn subscribe(&self, uri: impl Into<String>, peer: Peer<RoleServer>) {
        self.subscribers
            .lock()
            .await
            .entry(uri.into())
            .or_default()
            .push(peer);
    }

    pub async fn unsubscribe(&self, uri: &str) {
        self.subscribers.lock().await.remove(uri);
    }

    pub async fn notify_resource_updated(&self, uri: impl Into<String>) {
        let uri = uri.into();
        let peers = self
            .subscribers
            .lock()
            .await
            .get(&uri)
            .cloned()
            .unwrap_or_default();
        for peer in peers {
            let _ = peer
                .notify_resource_updated(ResourceUpdatedNotificationParam::new(uri.clone()))
                .await;
        }
    }
}
