use std::collections::BTreeSet;

use futures::StreamExt;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_artifact_contract::ArtifactId;
use veoveo_artifact_contract::ListArtifactsRequest;
use veoveo_mcp_contract::{ArtifactPlane, PlaneCaller};
use veoveo_platform_store::{
    ArtifactChange, ChangefeedConsumerId, ChangefeedDelivery, PlatformStore, PlatformTable,
};

#[path = "subscriptions/deadlines.rs"]
mod deadlines;
#[path = "subscriptions/listener.rs"]
mod listener;
pub(super) use listener::listen;

const SUBSCRIPTION_BUFFER: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SubscriptionKind {
    Index,
    Content(ArtifactId),
    Metadata(ArtifactId),
    Grants(ArtifactId),
}

#[derive(Clone, Copy)]
pub(super) enum ArtifactInvalidation {
    Reconcile,
    Changed(ArtifactId),
}

#[derive(Clone)]
pub(super) struct ArtifactSubscriptions {
    updates: broadcast::Sender<ArtifactInvalidation>,
    store: PlatformStore,
}

impl ArtifactSubscriptions {
    pub(super) fn new(store: PlatformStore) -> Self {
        let (updates, _) = broadcast::channel(SUBSCRIPTION_BUFFER);
        Self { updates, store }
    }
    pub(super) fn listen(&self) -> broadcast::Receiver<ArtifactInvalidation> {
        self.updates.subscribe()
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
                .map(|artifact| artifact.artifact_id()),
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
    consumer: ChangefeedConsumerId,
) -> anyhow::Result<()> {
    let cursor = store.changefeed_checkpoint(&consumer).await?;
    tokio::spawn(async move {
        let mut changes = store.observe_changes(
            vec![
                PlatformTable::ArtifactOccurrence,
                PlatformTable::ArtifactGrant,
                PlatformTable::ShareLink,
            ],
            cursor,
        );
        loop {
            let delivery = tokio::select! {
                _ = cancellation.cancelled() => return,
                delivery = changes.next() => match delivery {
                    Some(Ok(delivery)) => delivery,
                    Some(Err(error)) => {
                        tracing::warn!(%error, "Artifact changefeed needs reconciliation");
                        let _ = subscriptions.updates.send(ArtifactInvalidation::Reconcile);
                        continue;
                    }
                    None => return,
                }
            };
            let cursor = delivery.cursor();
            match delivery {
                ChangefeedDelivery::Reconcile { .. } => {
                    let _ = subscriptions.updates.send(ArtifactInvalidation::Reconcile);
                }
                ChangefeedDelivery::Changes { entries, .. } => {
                    let mut ids = BTreeSet::new();
                    for entry in entries {
                        match ArtifactChange::decode(&entry) {
                            Ok(Some(change)) => {
                                ids.insert(change.artifact_id);
                            }
                            Ok(None) => {}
                            Err(error) => {
                                tracing::warn!(%error, "Artifact change requires a current-state baseline");
                                let _ = subscriptions.updates.send(ArtifactInvalidation::Reconcile);
                            }
                        }
                    }
                    for id in ids {
                        let _ = subscriptions
                            .updates
                            .send(ArtifactInvalidation::Changed(id));
                    }
                }
            }
            if let Err(error) = store.checkpoint_changes(&consumer, cursor).await {
                tracing::warn!(%error, "Artifact changefeed checkpoint was not persisted");
            }
        }
    });
    Ok(())
}
