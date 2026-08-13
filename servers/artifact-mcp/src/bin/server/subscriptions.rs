use std::{collections::BTreeSet, time::Duration};

use futures::StreamExt;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{ArtifactId, ArtifactPlane, ListArtifactsRequest, PlaneCaller};
use veoveo_platform_store::{LiveStream, OutboxEventRecord, PlatformStore, PlatformTable};

const OUTBOX_PAGE_SIZE: u32 = 1_000;
const RECONCILE_INTERVAL: Duration = Duration::from_secs(15);
const LIVE_RECONNECT_DELAY: Duration = Duration::from_secs(1);
const SUBSCRIPTION_BUFFER: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SubscriptionKind {
    Index,
    Content(ArtifactId),
    Metadata(ArtifactId),
    Grants(ArtifactId),
}

#[derive(Clone)]
pub(super) struct ArtifactSubscriptions {
    updates: broadcast::Sender<ArtifactId>,
}

impl Default for ArtifactSubscriptions {
    fn default() -> Self {
        let (updates, _) = broadcast::channel(SUBSCRIPTION_BUFFER);
        Self { updates }
    }
}

impl ArtifactSubscriptions {
    pub(super) fn listen(&self) -> broadcast::Receiver<ArtifactId> {
        self.updates.subscribe()
    }

    async fn notify_artifact(&self, artifact_id: ArtifactId) {
        let _ = self.updates.send(artifact_id);
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
    subscriptions: ArtifactSubscriptions,
    cancellation: CancellationToken,
) -> anyhow::Result<()> {
    let live = store
        .live::<OutboxEventRecord>(PlatformTable::OutboxEvent)
        .await?;
    let cursor = store.latest_outbox_sequence().await?;
    tokio::spawn(dispatch_loop(
        store,
        subscriptions,
        cancellation,
        cursor,
        live,
    ));
    Ok(())
}

async fn dispatch_loop(
    store: PlatformStore,
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
            if let Err(error) = drain_outbox(&store, &subscriptions, &mut cursor).await {
                tracing::warn!("artifact outbox replay failed: {error}");
            }
            continue;
        }

        if let Err(error) = drain_outbox(&store, &subscriptions, &mut cursor).await {
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
                subscriptions.notify_artifact(artifact_id).await;
            }
        }
        *cursor = page.next_sequence;
        if count < OUTBOX_PAGE_SIZE as usize {
            return Ok(());
        }
    }
}
