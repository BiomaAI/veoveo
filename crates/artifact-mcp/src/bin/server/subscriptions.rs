use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};

use futures::StreamExt;
use rmcp::{RoleServer, model::ResourceUpdatedNotificationParam, service::Peer};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{ArtifactId, ArtifactPlane, ListArtifactsRequest, PlaneCaller};
use veoveo_platform_store::{LiveStream, OutboxEventRecord, PlatformStore, PlatformTable};

const OUTBOX_PAGE_SIZE: u32 = 1_000;
const RECONCILE_INTERVAL: Duration = Duration::from_secs(15);
const LIVE_RECONNECT_DELAY: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SubscriptionKind {
    Index,
    Content(ArtifactId),
    Metadata(ArtifactId),
    Grants(ArtifactId),
}

#[derive(Clone)]
struct Subscriber {
    key: String,
    uri: String,
    kind: SubscriptionKind,
    caller: PlaneCaller,
    peer: Peer<RoleServer>,
    visible: Option<Arc<Mutex<BTreeSet<ArtifactId>>>>,
}

#[derive(Clone, Default)]
pub(super) struct ArtifactSubscriptions {
    entries: Arc<RwLock<HashMap<String, Vec<Subscriber>>>>,
}

impl ArtifactSubscriptions {
    pub(super) async fn subscribe(
        &self,
        uri: String,
        kind: SubscriptionKind,
        caller: PlaneCaller,
        peer: Peer<RoleServer>,
        visible: Option<BTreeSet<ArtifactId>>,
    ) {
        let key = subscriber_key(&caller);
        let entry = Subscriber {
            key: key.clone(),
            uri: uri.clone(),
            kind,
            caller,
            peer,
            visible: visible.map(|ids| Arc::new(Mutex::new(ids))),
        };
        let mut entries = self.entries.write().await;
        let subscribers = entries.entry(uri).or_default();
        subscribers.retain(|existing| existing.key != key);
        subscribers.push(entry);
    }

    pub(super) async fn unsubscribe(&self, uri: &str, caller: &PlaneCaller) {
        let key = subscriber_key(caller);
        let mut entries = self.entries.write().await;
        if let Some(subscribers) = entries.get_mut(uri) {
            subscribers.retain(|entry| entry.key != key);
            if subscribers.is_empty() {
                entries.remove(uri);
            }
        }
    }

    async fn snapshot(&self) -> Vec<Subscriber> {
        self.entries
            .read()
            .await
            .values()
            .flat_map(|entries| entries.iter().cloned())
            .collect()
    }

    async fn remove(&self, uri: &str, key: &str) {
        let mut entries = self.entries.write().await;
        if let Some(subscribers) = entries.get_mut(uri) {
            subscribers.retain(|entry| entry.key != key);
            if subscribers.is_empty() {
                entries.remove(uri);
            }
        }
    }

    pub(super) async fn notify_artifact(&self, plane: &HttpArtifactPlane, artifact_id: ArtifactId) {
        for subscriber in self.snapshot().await {
            let decision = match subscriber.kind {
                SubscriptionKind::Index => {
                    let current = match visible_ids(plane, &subscriber.caller).await {
                        Ok(current) => current,
                        Err(error) => {
                            tracing::warn!("artifact index subscription recheck failed: {error}");
                            continue;
                        }
                    };
                    let visible = subscriber
                        .visible
                        .as_ref()
                        .expect("index subscriptions carry a visible set");
                    let mut previous = visible.lock().await;
                    let changed = *previous != current || current.contains(&artifact_id);
                    *previous = current;
                    changed
                }
                SubscriptionKind::Content(id) | SubscriptionKind::Metadata(id)
                    if id == artifact_id =>
                {
                    plane.head(&subscriber.caller, &id).await.is_ok()
                }
                SubscriptionKind::Grants(id) if id == artifact_id => {
                    plane.list_grants(&subscriber.caller, &id).await.is_ok()
                }
                _ => false,
            };
            if !decision {
                if !matches!(subscriber.kind, SubscriptionKind::Index) {
                    self.remove(&subscriber.uri, &subscriber.key).await;
                }
                continue;
            }
            if let Err(error) = subscriber
                .peer
                .notify_resource_updated(ResourceUpdatedNotificationParam::new(
                    subscriber.uri.clone(),
                ))
                .await
            {
                tracing::debug!("artifact resource notification failed: {error}");
                self.remove(&subscriber.uri, &subscriber.key).await;
                continue;
            }
            if subscriber.kind == SubscriptionKind::Index
                && let Err(error) = subscriber.peer.notify_resource_list_changed().await
            {
                tracing::debug!("artifact resource-list notification failed: {error}");
                self.remove(&subscriber.uri, &subscriber.key).await;
            }
        }
    }
}

pub(super) async fn visible_ids(
    plane: &HttpArtifactPlane,
    caller: &PlaneCaller,
) -> Result<BTreeSet<ArtifactId>, veoveo_mcp_contract::ArtifactPlaneError> {
    let mut cursor = None;
    let mut ids = BTreeSet::new();
    loop {
        let page = plane
            .list(
                caller,
                ListArtifactsRequest {
                    cursor,
                    limit: Some(100),
                },
            )
            .await?;
        ids.extend(
            page.artifacts
                .into_iter()
                .map(|artifact| artifact.artifact_id),
        );
        match page.next_cursor {
            Some(next) if Some(next) != cursor => cursor = Some(next),
            _ => break,
        }
    }
    Ok(ids)
}

pub(super) async fn start_dispatcher(
    store: PlatformStore,
    plane: HttpArtifactPlane,
    subscriptions: ArtifactSubscriptions,
    cancellation: CancellationToken,
) -> anyhow::Result<()> {
    let live = store
        .live::<OutboxEventRecord>(PlatformTable::OutboxEvent)
        .await?;
    let cursor = store.latest_outbox_sequence().await?;
    tokio::spawn(dispatch_loop(
        store,
        plane,
        subscriptions,
        cancellation,
        cursor,
        live,
    ));
    Ok(())
}

async fn dispatch_loop(
    store: PlatformStore,
    plane: HttpArtifactPlane,
    subscriptions: ArtifactSubscriptions,
    cancellation: CancellationToken,
    mut cursor: i64,
    mut live: LiveStream<OutboxEventRecord>,
) {
    let mut reconcile = tokio::time::interval(RECONCILE_INTERVAL);
    reconcile.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let wake = tokio::select! {
            _ = cancellation.cancelled() => return,
            _ = reconcile.tick() => true,
            item = live.next() => match item {
                Some(Ok(_)) => true,
                Some(Err(error)) => {
                    tracing::warn!("artifact outbox LIVE stream failed: {error}");
                    false
                }
                None => false,
            }
        };
        if wake {
            if let Err(error) = drain_outbox(&store, &plane, &subscriptions, &mut cursor).await {
                tracing::warn!("artifact outbox replay failed: {error}");
            }
            continue;
        }

        if let Err(error) = drain_outbox(&store, &plane, &subscriptions, &mut cursor).await {
            tracing::warn!("artifact outbox gap replay failed: {error}");
        }
        tokio::select! {
            _ = cancellation.cancelled() => return,
            _ = tokio::time::sleep(LIVE_RECONNECT_DELAY) => {}
        }
        loop {
            match store
                .live::<OutboxEventRecord>(PlatformTable::OutboxEvent)
                .await
            {
                Ok(reconnected) => {
                    live = reconnected;
                    break;
                }
                Err(error) => {
                    tracing::warn!("artifact outbox LIVE reconnect failed: {error}");
                    tokio::select! {
                        _ = cancellation.cancelled() => return,
                        _ = tokio::time::sleep(LIVE_RECONNECT_DELAY) => {}
                    }
                }
            }
        }
    }
}

async fn drain_outbox(
    store: &PlatformStore,
    plane: &HttpArtifactPlane,
    subscriptions: &ArtifactSubscriptions,
    cursor: &mut i64,
) -> anyhow::Result<()> {
    loop {
        let page = store.read_outbox(*cursor, OUTBOX_PAGE_SIZE).await?;
        if page.events.is_empty() {
            return Ok(());
        }
        let count = page.events.len();
        for event in page.events {
            if event.aggregate_type == "artifact"
                && let Ok(artifact_id) = ArtifactId::parse(&event.aggregate_id)
            {
                subscriptions.notify_artifact(plane, artifact_id).await;
            }
        }
        *cursor = page.next_sequence;
        if count < OUTBOX_PAGE_SIZE as usize {
            return Ok(());
        }
    }
}

fn subscriber_key(caller: &PlaneCaller) -> String {
    format!(
        "{}:{}:{}",
        caller.identity.profile,
        caller
            .identity
            .principal
            .tenant
            .as_ref()
            .map_or("", |tenant| tenant.as_str()),
        caller.identity.principal.id
    )
}
