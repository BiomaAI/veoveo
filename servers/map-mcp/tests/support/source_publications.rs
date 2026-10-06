//! Map publishes immutable members; layer archival preserves their visibility.
use crate::{authoring, restart::DeploymentRestart, tools};
use anyhow::{Context, ensure};
use rmcp::{Peer, RoleClient};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use veoveo_map_mcp::contract::*;
use veoveo_mcp_conformance::knowledge_probes::{KnowledgeCreateDriver, KnowledgeProbeFuture};
use veoveo_types::ResourceUri;

pub struct Publications {
    peer: Peer<RoleClient>,
    layers: [FeatureLayerId; 2],
    changes: AtomicUsize,
    restart: Arc<DeploymentRestart>,
}

impl Publications {
    pub fn new(
        peer: Peer<RoleClient>,
        layers: [FeatureLayerId; 2],
        restart: Arc<DeploymentRestart>,
    ) -> Self {
        Self {
            peer,
            layers,
            changes: AtomicUsize::new(0),
            restart,
        }
    }
}

impl KnowledgeCreateDriver for Publications {
    fn create(&self) -> KnowledgeProbeFuture<'_, ResourceUri> {
        Box::pin(async move {
            let step = self.changes.fetch_add(1, Ordering::SeqCst);
            let id = self
                .layers
                .get(step)
                .context("publication fixture count exceeded")?;
            let layer = authoring::fixture_layer(&self.peer, id).await?;
            ensure!(
                layer.archived_at.is_none(),
                "publication fixture is archived"
            );
            let title = format!("Source conformance publication {id}");
            // Dispatch once: a lost receipt does not authorize another publication.
            // Cleanup archives the selected parent even when this call fails.
            let publication: LayerPublication = tools::call(
                &self.peer,
                authoring::tool("publish_feature_layer")?,
                &PublishFeatureLayerRequest {
                    layer_id: id.clone(),
                    expected_layer_revision: layer.revision,
                    title: Some(title.clone()),
                },
            )
            .await?;
            ensure!(
                &publication.layer_id == id
                    && publication.layer_revision == layer.revision
                    && publication.title.as_deref() == Some(title.as_str()),
                "publication receipt disagrees with the selected layer"
            );
            Ok(MapKnowledgeMember::Publication {
                layer: id.clone(),
                publication: publication.into_value().publication_id,
            }
            .to_uri())
        })
    }

    fn restart(&self) -> KnowledgeProbeFuture<'_> {
        Box::pin(self.restart.restart())
    }
}
