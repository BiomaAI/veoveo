//! Cache validity follows an authenticated catalog stream and the authority epoch.
use super::{GatewayMcp, discovery::DiscoveryCacheKey};
use crate::{
    AuthenticatedSubject,
    mcp_support::{mcp_internal, upstream_error},
};
use rmcp::{
    model::{ErrorData as McpError, ServerNotification, SubscriptionFilter},
    service::{Peer, RoleClient, RoleServer, Subscription},
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::{Mutex, OnceCell};
use tokio_util::sync::CancellationToken;

const MAX_WATCHES: usize = 256;
const OPEN_DEADLINE: Duration = Duration::from_secs(10);
#[derive(Debug, Default)]
pub(super) struct DiscoveryWatches(Mutex<BTreeMap<DiscoveryCacheKey, Arc<OnceCell<Watch>>>>);
#[derive(Debug)]
struct Watch {
    cancelled: CancellationToken,
}
impl Drop for Watch {
    fn drop(&mut self) {
        self.cancelled.cancel();
    }
}
impl GatewayMcp {
    pub(super) async fn discovery_watch_active(&self, key: &DiscoveryCacheKey) -> bool {
        let snapshot = self.catalog.snapshot();
        if snapshot.generation() != key.catalog_generation {
            return false;
        }
        let Some(manifest) = snapshot.catalog().server(&key.server) else {
            return false;
        };
        if !manifest.capabilities.tools_list_changed
            && !manifest.capabilities.resources_list_changed
            && !manifest.capabilities.prompts_list_changed
        {
            return true;
        }
        self.discovery_watches
            .0
            .lock()
            .await
            .get(key)
            .and_then(|cell| cell.get())
            .is_some_and(|watch| !watch.cancelled.is_cancelled())
    }
    pub(super) async fn ensure_discovery_watch(
        &self,
        key: &DiscoveryCacheKey,
        downstream: Peer<RoleServer>,
        subject: &AuthenticatedSubject,
    ) -> Result<(), McpError> {
        let snapshot = self.catalog.snapshot();
        if snapshot.generation() != key.catalog_generation {
            return Err(mcp_internal("catalog changed during discovery; retry"));
        }
        let manifest = snapshot
            .catalog()
            .server(&key.server)
            .ok_or_else(|| mcp_internal("unknown catalog server"))?;
        let indexing = snapshot
            .catalog()
            .oauth_client(&subject.access_token.oauth_client_id)
            .is_some_and(|client| client.knowledge_indexing.is_some());
        let mut filter = SubscriptionFilter::new();
        filter.tools_list_changed = manifest.capabilities.tools_list_changed.then_some(true);
        filter.resources_list_changed =
            manifest.capabilities.resources_list_changed.then_some(true);
        filter.prompts_list_changed = manifest.capabilities.prompts_list_changed.then_some(true);
        if filter.tools_list_changed.is_none()
            && filter.resources_list_changed.is_none()
            && filter.prompts_list_changed.is_none()
        {
            return Ok(());
        }
        let cell = {
            let mut watches = self.discovery_watches.0.lock().await;
            watches.retain(|watch_key, cell| {
                watch_key.catalog_generation == key.catalog_generation
                    && cell
                        .get()
                        .is_none_or(|watch| !watch.cancelled.is_cancelled())
            });
            if let Some(cell) = watches.get(key) {
                cell.clone()
            } else {
                if watches.len() >= MAX_WATCHES {
                    return Err(mcp_internal("catalog subscription capacity reached"));
                }
                let cell = Arc::new(OnceCell::new());
                watches.insert(key.clone(), cell.clone());
                cell
            }
        };
        let initialized = cell.get_or_try_init(|| async {
            let remaining = (subject.access_token.expires_at - chrono::Utc::now()).to_std()
                .map_err(|_| mcp_internal("catalog authority expired"))?;
            let deadline = tokio::time::Instant::now() + remaining;
            let mut generation = self.catalog.subscribe();
            let (connection, mut subscription) = tokio::time::timeout(OPEN_DEADLINE, async {
                let connection = self.upstream(&key.server, downstream, subject).await.map_err(super::http_response::RequestError::into_protocol)?;
                let subscription = open_catalog_subscription(&connection.peer, &filter, indexing).await?;
                Ok::<_,McpError>((connection, subscription))
            }).await.map_err(|_| mcp_internal("catalog subscription open deadline exceeded"))??;
            if *generation.borrow_and_update() != key.catalog_generation { return Err(mcp_internal("catalog changed while opening subscription")); }
            // A new stream cannot recover notifications from before registration.
            // Rebuild this key after the stream is listening, closing that gap.
            self.discovery.invalidate_cached_key(key).await;
            let cancelled = CancellationToken::new();
            let stop = cancelled.clone();
            let cache = Arc::downgrade(&self.discovery);
            let key = key.clone();
            tokio::spawn(async move {
                let _connection = connection;
                loop {
                    tokio::select! {
                        _ = stop.cancelled() => break,
                        _ = tokio::time::sleep_until(deadline) => break,
                        _ = generation.changed() => break,
                        next = subscription.next() => {
                            let Some(cache) = cache.upgrade() else { break; };
                            match next {
                                Ok(Some(ServerNotification::ToolListChangedNotification(_))) => cache.invalidate_tools(&key.server).await,
                                Ok(Some(ServerNotification::ResourceListChangedNotification(_))) => cache.invalidate_resource_surfaces(&key.server).await,
                                Ok(Some(ServerNotification::PromptListChangedNotification(_))) => cache.invalidate_prompts(&key.server).await,
                                Ok(Some(_)) => {},
                                Ok(None) | Err(_) => break,
                            }
                        }
                    }
                }
                if let Some(cache) = cache.upgrade() { cache.invalidate_key(&key).await; }
                stop.cancel();
            });
            Ok::<_,McpError>(Watch { cancelled })
        }).await;
        if let Err(error) = initialized {
            let mut watches = self.discovery_watches.0.lock().await;
            if watches
                .get(key)
                .is_some_and(|current| Arc::ptr_eq(current, &cell))
            {
                watches.remove(key);
            }
            return Err(error);
        }
        if initialized.unwrap().cancelled.is_cancelled() {
            return Err(mcp_internal("catalog subscription ended; retry"));
        }
        Ok(())
    }
}

pub(super) async fn open_catalog_subscription(
    peer: &Peer<RoleClient>,
    filter: &SubscriptionFilter,
    indexing: bool,
) -> Result<Subscription, McpError> {
    let mut subscription = peer.listen(filter.clone()).await.map_err(upstream_error)?;
    if subscription.acknowledged() != filter {
        return Err(mcp_internal("upstream narrowed its catalog subscription"));
    }
    let source = peer
        .peer_info()
        .is_some_and(|info| veoveo_mcp_knowledge_extension::client::supports(&info.capabilities));
    // K07's initial notification establishes observation for every caller. Leaving
    // it queued races the first catalog fetch and invalidates unrelated callers.
    if indexing || source {
        super::subscriptions::wait_for_source_baseline(&mut subscription, filter).await?;
    }
    Ok(subscription)
}
