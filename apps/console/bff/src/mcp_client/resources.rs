//! Auth-scoped resource subscription sharing and bounded listener cleanup.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResourceCapacity {
    pub(crate) max_upstream_listeners: usize,
    pub(crate) max_downstream_subscriptions: usize,
}

impl Default for ResourceCapacity {
    fn default() -> Self {
        Self {
            max_upstream_listeners: 64,
            max_downstream_subscriptions: 256,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ResourceSubscription {
    pub(crate) receiver: broadcast::Receiver<String>,
    pub(crate) newly_registered: bool,
}

#[derive(Default)]
pub(super) struct ResourceSubscriptions {
    by_id: BTreeMap<Uuid, String>,
    pending_by_id: BTreeMap<Uuid, String>,
    counts_by_uri: BTreeMap<String, usize>,
    pending_uris: std::collections::BTreeSet<String>,
    listeners_by_uri: BTreeMap<String, ResourceListener>,
}

// A timed-out HTTP request must return its pending capacity, even before listen responds.
struct PendingRegistration {
    subscriptions: Arc<Mutex<ResourceSubscriptions>>,
    id: Uuid,
    uri: String,
    armed: bool,
}
impl Drop for PendingRegistration {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let subscriptions = self.subscriptions.clone();
        let id = self.id;
        let uri = self.uri.clone();
        tokio::spawn(async move {
            let mut state = subscriptions.lock().await;
            if state.pending_by_id.get(&id) == Some(&uri) {
                state.pending_by_id.remove(&id);
                if !state.pending_by_id.values().any(|value| value == &uri) {
                    state.pending_uris.remove(&uri);
                }
            }
        });
    }
}

#[derive(Debug)]
pub(crate) enum ResourceSubscriptionError {
    Capacity {
        resource: &'static str,
        limit: usize,
    },
    IdentityConflict,
    NotAdmitted,
    ClientClosing,
    Upstream(anyhow::Error),
}

impl ResourceSubscriptionError {
    #[cfg(test)]
    pub(crate) const fn capacity(&self) -> Option<(&'static str, usize)> {
        match self {
            Self::Capacity { resource, limit } => Some((resource, *limit)),
            _ => None,
        }
    }
}

impl std::fmt::Display for ResourceSubscriptionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Capacity { resource, limit } => {
                write!(
                    formatter,
                    "MCP resource {resource} capacity {limit} is exhausted"
                )
            }
            Self::IdentityConflict => formatter
                .write_str("MCP resource subscription identity is already bound to another URI"),
            Self::ClientClosing => formatter.write_str("auth-scoped MCP client is closing"),
            Self::NotAdmitted => {
                formatter.write_str("the upstream did not admit the exact resource subscription")
            }
            Self::Upstream(error) => write!(formatter, "{error:#}"),
        }
    }
}

impl std::error::Error for ResourceSubscriptionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Upstream(error) => error.source(),
            _ => None,
        }
    }
}

pub(super) struct ResourceListener {
    pub(super) cancel: oneshot::Sender<()>,
    pub(super) stopped: oneshot::Receiver<()>,
}

impl AuthScopedMcpClient {
    #[cfg(test)]
    pub(super) async fn pending_resource_admissions(&self) -> usize {
        self.resource_subscriptions.lock().await.pending_by_id.len()
    }
    pub(super) fn resources_current(&self) -> bool {
        !self.shutting_down.load(Ordering::Acquire) && *self.resource_sources_current.borrow()
    }

    /// Loss is monotonic for this auth-scoped client. Reconnect obtains a new client.
    pub(crate) async fn resource_source_lost(&self) {
        let mut current = self.resource_sources_current.subscribe();
        loop {
            if !*current.borrow_and_update() {
                return;
            }
            if current.changed().await.is_err() {
                return;
            }
        }
    }
    /// Register one browser resource subscription on the auth-scoped MCP client.
    /// EventSource reconnects reuse the same UUID and therefore do not add
    /// another upstream subscription or reference count.
    pub(crate) async fn subscribe_resource(
        &self,
        subscription_id: Uuid,
        uri: String,
    ) -> Result<ResourceSubscription, ResourceSubscriptionError> {
        if !self.resources_current() {
            return Err(ResourceSubscriptionError::ClientClosing);
        }
        let receiver = self.resource_updates.subscribe();
        let uri_lock = self.resource_subscription_lock(&uri).await;
        let _uri_guard = uri_lock.lock().await;
        let mut subscriptions = self.resource_subscriptions.lock().await;
        if !self.resources_current() {
            return Err(ResourceSubscriptionError::ClientClosing);
        }
        if let Some(existing) = subscriptions.by_id.get(&subscription_id) {
            if existing != &uri {
                return Err(ResourceSubscriptionError::IdentityConflict);
            }
            return Ok(ResourceSubscription {
                receiver,
                newly_registered: false,
            });
        }
        if subscriptions.pending_by_id.contains_key(&subscription_id) {
            return Err(ResourceSubscriptionError::IdentityConflict);
        }
        if subscriptions.by_id.len() + subscriptions.pending_by_id.len()
            >= self.resource_capacity.max_downstream_subscriptions
        {
            return Err(ResourceSubscriptionError::Capacity {
                resource: "downstream_subscriptions",
                limit: self.resource_capacity.max_downstream_subscriptions,
            });
        }
        let first_for_uri = !subscriptions.counts_by_uri.contains_key(&uri);
        if !first_for_uri {
            subscriptions.by_id.insert(subscription_id, uri.clone());
            *subscriptions.counts_by_uri.entry(uri).or_default() += 1;
            return Ok(ResourceSubscription {
                receiver,
                newly_registered: true,
            });
        }
        if subscriptions.listeners_by_uri.len() + subscriptions.pending_uris.len()
            >= self.resource_capacity.max_upstream_listeners
        {
            return Err(ResourceSubscriptionError::Capacity {
                resource: "upstream_listeners",
                limit: self.resource_capacity.max_upstream_listeners,
            });
        }
        subscriptions
            .pending_by_id
            .insert(subscription_id, uri.clone());
        subscriptions.pending_uris.insert(uri.clone());
        let mut registration = PendingRegistration {
            subscriptions: self.resource_subscriptions.clone(),
            id: subscription_id,
            uri: uri.clone(),
            armed: true,
        };
        drop(subscriptions);
        let filter = SubscriptionFilter::builder()
            .resource_subscription(uri.clone())
            .build();
        let listener = self
            .service
            .listen(filter.clone())
            .await
            .context("opening Console MCP resource listener");
        let mut listener = match listener {
            Ok(listener) => listener,
            Err(error) => {
                let mut subscriptions = self.resource_subscriptions.lock().await;
                registration.armed = false;
                subscriptions.pending_by_id.remove(&subscription_id);
                subscriptions.pending_uris.remove(&uri);
                return Err(ResourceSubscriptionError::Upstream(error));
            }
        };
        if listener.acknowledged() != &filter {
            let _ = tokio::time::timeout(Duration::from_secs(2), listener.cancel()).await;
            return Err(ResourceSubscriptionError::NotAdmitted);
        }
        let (cancel_tx, mut cancel_rx) = oneshot::channel();
        let (stopped_tx, stopped_rx) = oneshot::channel();
        let resource_updates = self.resource_updates.clone();
        let source_current = self.resource_sources_current.clone();
        let catalog_revision = self.catalog_revision.clone();
        let catalog_updates = self.catalog_updates.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = &mut cancel_rx => {
                        let _ = listener.cancel().await;
                        break;
                    }
                    notification = listener.next() => match notification {
                        Ok(Some(ServerNotification::ResourceUpdatedNotification(update))) => {
                            let _ = resource_updates.send(update.params.uri);
                        }
                        Ok(Some(ServerNotification::ResourceListChangedNotification(_))) => {
                            publish_catalog_change(&catalog_revision, &catalog_updates);
                        }
                        Ok(Some(_)) => {}
                        Ok(None) | Err(_) => {
                            source_current.send_replace(false);
                            break;
                        },
                    }
                }
            }
            let _ = stopped_tx.send(());
        });
        subscriptions = self.resource_subscriptions.lock().await;
        registration.armed = false;
        subscriptions.pending_by_id.remove(&subscription_id);
        subscriptions.pending_uris.remove(&uri);
        if !self.resources_current() {
            drop(subscriptions);
            Self::stop_resource_listener(ResourceListener {
                cancel: cancel_tx,
                stopped: stopped_rx,
            })
            .await
            .map_err(ResourceSubscriptionError::Upstream)?;
            return Err(ResourceSubscriptionError::ClientClosing);
        }
        if let Some(existing) = subscriptions.by_id.get(&subscription_id) {
            let same_uri = existing == &uri;
            drop(subscriptions);
            Self::stop_resource_listener(ResourceListener {
                cancel: cancel_tx,
                stopped: stopped_rx,
            })
            .await
            .map_err(ResourceSubscriptionError::Upstream)?;
            if !same_uri {
                return Err(ResourceSubscriptionError::IdentityConflict);
            }
            return Ok(ResourceSubscription {
                receiver,
                newly_registered: false,
            });
        }
        subscriptions.listeners_by_uri.insert(
            uri.clone(),
            ResourceListener {
                cancel: cancel_tx,
                stopped: stopped_rx,
            },
        );
        subscriptions.by_id.insert(subscription_id, uri.clone());
        *subscriptions.counts_by_uri.entry(uri).or_default() += 1;
        Ok(ResourceSubscription {
            receiver,
            newly_registered: true,
        })
    }

    pub(super) async fn shutdown(&self) {
        if self.shutting_down.swap(true, Ordering::AcqRel) {
            return;
        }
        self.resource_sources_current.send_replace(false);
        let catalog_listener = self.catalog_listener.lock().await.take();
        let listeners = {
            let mut subscriptions = self.resource_subscriptions.lock().await;
            subscriptions.by_id.clear();
            subscriptions.pending_by_id.clear();
            subscriptions.counts_by_uri.clear();
            subscriptions.pending_uris.clear();
            std::mem::take(&mut subscriptions.listeners_by_uri)
                .into_values()
                .collect::<Vec<_>>()
        };
        futures::future::join_all(listeners.into_iter().map(Self::stop_resource_listener)).await;
        if let Some(listener) = catalog_listener {
            let _ = Self::stop_resource_listener(listener).await;
        }
        self.service.cancellation_token().cancel();
    }

    /// Release one resource subscription. Multiple tabs sharing the same Console
    /// MCP client retain the one upstream subscription until the final UUID
    /// closes.
    pub(crate) async fn unsubscribe_resource(&self, subscription_id: Uuid) -> anyhow::Result<()> {
        let Some(uri) = self
            .resource_subscriptions
            .lock()
            .await
            .by_id
            .get(&subscription_id)
            .cloned()
        else {
            return Ok(());
        };
        let uri_lock = self.resource_subscription_lock(&uri).await;
        let _uri_guard = uri_lock.lock().await;
        let mut subscriptions = self.resource_subscriptions.lock().await;
        let Some(current_uri) = subscriptions.by_id.get(&subscription_id) else {
            return Ok(());
        };
        anyhow::ensure!(
            current_uri == &uri,
            "app resource subscription identity changed URI while unsubscribing"
        );
        let final_for_uri = subscriptions.counts_by_uri.get(&uri).copied() == Some(1);
        let listener = final_for_uri
            .then(|| subscriptions.listeners_by_uri.remove(&uri))
            .flatten();
        subscriptions.by_id.remove(&subscription_id);
        if final_for_uri {
            subscriptions.counts_by_uri.remove(&uri);
        } else if let Some(count) = subscriptions.counts_by_uri.get_mut(&uri) {
            *count -= 1;
        }
        drop(subscriptions);
        if let Some(listener) = listener {
            Self::stop_resource_listener(listener).await?;
        }
        Ok(())
    }

    pub(super) async fn stop_resource_listener(listener: ResourceListener) -> anyhow::Result<()> {
        let _ = listener.cancel.send(());
        tokio::time::timeout(Duration::from_secs(2), listener.stopped)
            .await
            .context("timed out stopping Console MCP resource listener")?
            .context("Console MCP resource listener stopped without acknowledgement")
    }

    async fn resource_subscription_lock(&self, uri: &str) -> Arc<Mutex<()>> {
        self.resource_subscription_locks
            .lock()
            .await
            .entry(uri.to_owned())
            .or_default()
            .clone()
    }
}
