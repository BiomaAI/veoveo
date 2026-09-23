//! Request-scoped final-profile subscription fan-out.

use std::future::pending;

use rmcp::{
    ErrorData,
    model::SubscriptionFilter,
    service::{SubscriptionContext, SubscriptionSendError},
};
use tokio::sync::broadcast;

const SUBSCRIPTION_BUFFER: usize = 256;

fn send_error(error: SubscriptionSendError) -> ErrorData {
    ErrorData::internal_error(error.to_string(), None)
}

/// Process-wide resource update broadcaster.
///
/// The broadcaster carries facts, not peers or authorization state. Each
/// `subscriptions/listen` request validates its own filter and owns its sink.
#[derive(Clone, Debug)]
pub enum ResourceUpdate {
    Uri(String),
    Reconcile,
}

pub struct SubscriptionHub {
    updates: broadcast::Sender<ResourceUpdate>,
    list_changes: broadcast::Sender<()>,
}

impl Default for SubscriptionHub {
    fn default() -> Self {
        let (updates, _) = broadcast::channel(SUBSCRIPTION_BUFFER);
        let (list_changes, _) = broadcast::channel(SUBSCRIPTION_BUFFER);
        Self {
            updates,
            list_changes,
        }
    }
}

impl SubscriptionHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn listen(&self) -> broadcast::Receiver<ResourceUpdate> {
        self.updates.subscribe()
    }

    pub fn listen_resource_list_changes(&self) -> broadcast::Receiver<()> {
        self.list_changes.subscribe()
    }

    pub async fn notify_resource_updated(&self, uri: impl Into<String>) {
        let _ = self.updates.send(ResourceUpdate::Uri(uri.into()));
    }

    /// Content changes affect only each listener's already-authorized identities.
    /// They do not change the resource inventory or its discovery metadata.
    pub async fn notify_resource_contents_changed(&self) {
        let _ = self.updates.send(ResourceUpdate::Reconcile);
    }

    /// Reconcile both resource contents and the discovery inventory.
    pub async fn notify_resources_changed(&self) {
        self.notify_resource_contents_changed().await;
        self.notify_resource_list_changed().await;
    }

    pub async fn notify_resource_list_changed(&self) {
        let _ = self.list_changes.send(());
    }
}

/// Resource-list change broadcaster used where list mutations originate in a
/// worker that does not otherwise share the server's resource hub.
pub struct ResourceListObservers {
    changes: broadcast::Sender<()>,
}

impl Default for ResourceListObservers {
    fn default() -> Self {
        let (changes, _) = broadcast::channel(SUBSCRIPTION_BUFFER);
        Self { changes }
    }
}

impl ResourceListObservers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn listen(&self) -> broadcast::Receiver<()> {
        self.changes.subscribe()
    }

    pub async fn notify_changed(&self) {
        let _ = self.changes.send(());
    }
}

pub async fn receive_resource_update(
    receiver: &mut Option<broadcast::Receiver<ResourceUpdate>>,
) -> ResourceUpdate {
    loop {
        let Some(receiver) = receiver.as_mut() else {
            pending::<()>().await;
            unreachable!();
        };
        match receiver.recv().await {
            Ok(uri) => return uri,
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "subscription resource updates lagged");
                return ResourceUpdate::Reconcile;
            }
            Err(broadcast::error::RecvError::Closed) => pending::<()>().await,
        }
    }
}

pub async fn receive_resource_list_change(receiver: &mut Option<broadcast::Receiver<()>>) {
    loop {
        let Some(receiver) = receiver.as_mut() else {
            pending::<()>().await;
            unreachable!();
        };
        match receiver.recv().await {
            Ok(()) => return,
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "subscription resource-list updates lagged");
                return;
            }
            Err(broadcast::error::RecvError::Closed) => pending::<()>().await,
        }
    }
}

/// Runs a resource-only final-profile listener after the caller has validated
/// every accepted URI.
pub async fn listen_resources(
    context: SubscriptionContext,
    resources: &SubscriptionHub,
    extra_list_changes: Option<&ResourceListObservers>,
) -> Result<(), ErrorData> {
    let accepted = context.accepted().clone();
    let mut updates = Some(resources.listen());
    let mut hub_lists = Some(resources.listen_resource_list_changes());
    let mut extra_lists = extra_list_changes.map(ResourceListObservers::listen);
    loop {
        tokio::select! {
            () = context.cancelled() => return Ok(()),
            uri = receive_resource_update(&mut updates) => {
                send_resource_update(&context, uri).await?;
            }
            () = receive_resource_list_change(&mut hub_lists), if accepted.resources_list_changed == Some(true) => {
                context.sink().notify_resource_list_changed().await.map_err(send_error)?;
            }
            () = receive_resource_list_change(&mut extra_lists), if accepted.resources_list_changed == Some(true) => {
                context.sink().notify_resource_list_changed().await.map_err(send_error)?;
            }
        }
    }
}

/// Lets the SDK intersect a server's request with its advertised capability
/// set while retaining only final `subscriptions/listen` fields.
pub fn accepted_subscription_filter(requested: &SubscriptionFilter) -> Option<SubscriptionFilter> {
    Some(requested.clone())
}

/// Notifications carry only identities already admitted by this request.
pub async fn send_resource_update(
    context: &SubscriptionContext,
    update: ResourceUpdate,
) -> Result<(), ErrorData> {
    for uri in context.accepted().resource_subscriptions.iter().flatten() {
        if matches!(&update, ResourceUpdate::Reconcile)
            || matches!(&update, ResourceUpdate::Uri(changed) if changed == uri)
        {
            context
                .sink()
                .notify_resource_updated(uri.clone())
                .await
                .map_err(send_error)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn content_updates_do_not_invalidate_catalog_discovery() {
        let hub = SubscriptionHub::new();
        let mut updates = hub.listen();
        let mut lists = hub.listen_resource_list_changes();
        hub.notify_resource_contents_changed().await;
        assert!(matches!(
            updates.recv().await.unwrap(),
            ResourceUpdate::Reconcile
        ));
        assert!(matches!(
            lists.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        hub.notify_resources_changed().await;
        assert!(matches!(
            updates.recv().await.unwrap(),
            ResourceUpdate::Reconcile
        ));
        lists.recv().await.unwrap();
    }

    #[tokio::test]
    async fn slow_resource_listeners_reconcile_their_accepted_identities() {
        let hub = SubscriptionHub::new();
        let mut receiver = Some(hub.listen());
        let mut lists = Some(hub.listen_resource_list_changes());
        for _ in 0..300 {
            hub.notify_resource_updated("fixture://changed").await;
            hub.notify_resource_list_changed().await;
        }
        assert!(matches!(
            receive_resource_update(&mut receiver).await,
            ResourceUpdate::Reconcile
        ));
        receive_resource_list_change(&mut lists).await;
    }
}
