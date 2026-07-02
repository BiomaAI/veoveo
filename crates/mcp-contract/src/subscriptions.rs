use std::collections::{BTreeMap, HashMap};

use rmcp::{RoleServer, model::ResourceUpdatedNotificationParam, service::Peer};
use tokio::sync::Mutex;

use crate::PrincipalId;

type ResourceSubscriptionMap = HashMap<String, PrincipalSubscriptions>;
type PrincipalSubscriptions = BTreeMap<PrincipalId, Vec<Peer<RoleServer>>>;

/// In-memory resource subscription registry keyed by resource URI and principal.
#[derive(Default)]
pub struct SubscriptionHub {
    subscribers: Mutex<ResourceSubscriptionMap>,
}

impl SubscriptionHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn subscribe(
        &self,
        uri: impl Into<String>,
        principal: PrincipalId,
        peer: Peer<RoleServer>,
    ) {
        self.subscribers
            .lock()
            .await
            .entry(uri.into())
            .or_default()
            .entry(principal)
            .or_default()
            .push(peer);
    }

    pub async fn unsubscribe(&self, uri: &str, principal: &PrincipalId) {
        let mut subscribers = self.subscribers.lock().await;
        if let Some(uri_subscribers) = subscribers.get_mut(uri) {
            uri_subscribers.remove(principal);
            if uri_subscribers.is_empty() {
                subscribers.remove(uri);
            }
        }
    }

    pub async fn notify_resource_updated(&self, uri: impl Into<String>) {
        let uri = uri.into();
        let peers = self
            .subscribers
            .lock()
            .await
            .get(&uri)
            .map(|uri_subscribers| {
                uri_subscribers
                    .values()
                    .flat_map(|peers| peers.iter().cloned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for peer in peers {
            let _ = peer
                .notify_resource_updated(ResourceUpdatedNotificationParam::new(uri.clone()))
                .await;
        }
    }
}
