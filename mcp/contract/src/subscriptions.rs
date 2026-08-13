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
pub struct SubscriptionHub {
    updates: broadcast::Sender<String>,
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

    pub fn listen(&self) -> broadcast::Receiver<String> {
        self.updates.subscribe()
    }

    pub fn listen_resource_list_changes(&self) -> broadcast::Receiver<()> {
        self.list_changes.subscribe()
    }

    pub async fn notify_resource_updated(&self, uri: impl Into<String>) {
        let _ = self.updates.send(uri.into());
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

pub async fn receive_resource_update(receiver: &mut Option<broadcast::Receiver<String>>) -> String {
    loop {
        let Some(receiver) = receiver.as_mut() else {
            pending::<()>().await;
            unreachable!();
        };
        match receiver.recv().await {
            Ok(uri) => return uri,
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "subscription resource updates lagged");
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
                if accepted.resource_subscriptions.as_ref().is_some_and(|uris| uris.contains(&uri)) {
                    context.sink().notify_resource_updated(uri).await.map_err(send_error)?;
                }
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
