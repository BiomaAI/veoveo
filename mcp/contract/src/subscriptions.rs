use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

use futures::{StreamExt, stream::FuturesUnordered};
use rmcp::{RoleServer, model::ResourceUpdatedNotificationParam, service::Peer};
use tokio::sync::Mutex;

use crate::PrincipalId;

type ResourceSubscriptionMap = HashMap<String, PrincipalSubscriptions>;
type PrincipalSubscriptions = BTreeMap<PrincipalId, Vec<Peer<RoleServer>>>;

const NOTIFICATION_DELIVERY_TIMEOUT: Duration = Duration::from_secs(2);

/// Delivers a resource-list change through the session that owns `peer`.
///
/// Delivery is awaited to preserve protocol ordering. The timeout bounds
/// backpressure without detaching work from the session lifecycle.
pub async fn notify_resource_list_changed(peer: &Peer<RoleServer>) {
    match tokio::time::timeout(
        NOTIFICATION_DELIVERY_TIMEOUT,
        peer.notify_resource_list_changed(),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::warn!(%error, "failed to deliver MCP resource-list change");
        }
        Err(_) => {
            tracing::warn!("timed out delivering MCP resource-list change");
        }
    }
}

/// Sessions that have observed a dynamic resource list since its last change.
///
/// A list-change notification consumes the observation. A conforming client
/// lists again after receiving the signal, which registers the session for
/// the next change and prevents stale peers from accumulating indefinitely.
#[derive(Default)]
pub struct ResourceListObservers {
    peers: Mutex<Vec<Peer<RoleServer>>>,
}

impl ResourceListObservers {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn observe(&self, peer: Peer<RoleServer>) {
        self.peers.lock().await.push(peer);
    }

    pub async fn notify_changed(&self) {
        let peers = std::mem::take(&mut *self.peers.lock().await);
        let mut deliveries = peers
            .into_iter()
            .map(|peer| async move { notify_resource_list_changed(&peer).await })
            .collect::<FuturesUnordered<_>>();
        while deliveries.next().await.is_some() {}
    }
}

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
        let mut deliveries = peers
            .into_iter()
            .map(|peer| {
                let uri = uri.clone();
                async move {
                    match tokio::time::timeout(
                        NOTIFICATION_DELIVERY_TIMEOUT,
                        peer.notify_resource_updated(ResourceUpdatedNotificationParam::new(
                            uri.clone(),
                        )),
                    )
                    .await
                    {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => {
                            tracing::warn!(%uri, %error, "failed to deliver MCP resource update");
                        }
                        Err(_) => {
                            tracing::warn!(%uri, "timed out delivering MCP resource update");
                        }
                    }
                }
            })
            .collect::<FuturesUnordered<_>>();
        while deliveries.next().await.is_some() {}
    }
}
