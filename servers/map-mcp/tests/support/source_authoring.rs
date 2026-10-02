//! Map-owned mutation recipes. The shared runner owns protocol assertions.
use crate::{installed, restart::DeploymentRestart, tools};
use anyhow::{Result, ensure};
use rmcp::{Peer, RoleClient};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use veoveo_map_mcp::contract::*;
use veoveo_mcp_conformance::knowledge_probes::{KnowledgeChangeDriver, KnowledgeProbeFuture};
use veoveo_mcp_contract::GatewayToolName;

pub enum Mutation {
    Layer(FeatureLayerId),
    Feature {
        layer: FeatureLayerId,
        feature: MapFeatureId,
    },
}

pub struct Authoring {
    peer: Peer<RoleClient>,
    mutation: Mutation,
    changes: AtomicUsize,
    restart: Arc<DeploymentRestart>,
}

impl Authoring {
    pub fn new(
        peer: Peer<RoleClient>,
        mutation: Mutation,
        restart: Arc<DeploymentRestart>,
    ) -> Self {
        Self {
            peer,
            mutation,
            restart,
            changes: AtomicUsize::new(0),
        }
    }

    pub async fn layer(&self, id: &FeatureLayerId) -> Result<FeatureLayer> {
        fixture_layer(&self.peer, id).await
    }

    pub async fn archive(&self, id: &FeatureLayerId) -> Result<()> {
        let layer = self.layer(id).await?;
        if layer.archived_at.is_some() {
            return Ok(());
        }
        let result: FeatureLayer = tools::call(
            &self.peer,
            tool("archive_feature_layer")?,
            &ArchiveFeatureLayerRequest {
                layer_id: id.clone(),
                expected_layer_revision: layer.revision,
            },
        )
        .await?;
        ensure!(
            &result.layer_id == id && result.archived_at.is_some(),
            "Map archive receipt disagrees"
        );
        ensure!(
            self.layer(id).await?.archived_at.is_some(),
            "Map layer remains active after archive"
        );
        Ok(())
    }

    async fn update_feature(
        &self,
        layer: &FeatureLayerId,
        feature: &MapFeatureId,
        step: usize,
    ) -> Result<()> {
        let parent = self.layer(layer).await?;
        let member = MapKnowledgeMember::Feature {
            layer: layer.clone(),
            feature: feature.clone(),
        };
        let before: MapFeature = installed::read(&self.peer, &member.source_uri()).await?;
        ensure!(
            &before.id == feature && &before.layer_id == layer && !before.deleted,
            "feature does not belong to the selected live fixture layer"
        );
        let title = format!("Source conformance feature step {step}");
        let result: CommitFeatureChangesOutput = tools::call(
            &self.peer,
            tool("commit_feature_changes")?,
            &CommitFeatureChangesRequest {
                layer_id: layer.clone(),
                expected_layer_revision: parent.revision,
                idempotency_key: format!("source-conformance-{}", uuid::Uuid::now_v7()),
                mutations: vec![FeatureMutation::Replace {
                    feature_id: feature.clone(),
                    expected_feature_revision: before.feature_revision,
                    feature: FeatureInput {
                        feature_id: Some(feature.clone()),
                        geometry: before.geometry,
                        properties: before.properties,
                        semantic_type: before.semantic_type,
                        time: before.time,
                        title: Some(title.clone()),
                        related_resources: before.related_resources,
                        evidence_resources: before.evidence_resources,
                    },
                }],
            },
        )
        .await?;
        ensure!(
            result.features.len() == 1
                && result.features[0].id == *feature
                && result.features[0].title.as_deref() == Some(title.as_str()),
            "feature replacement receipt disagrees"
        );
        Ok(())
    }
}

impl KnowledgeChangeDriver for Authoring {
    fn mutate(&self) -> KnowledgeProbeFuture<'_> {
        Box::pin(async move {
            let step = self.changes.fetch_add(1, Ordering::SeqCst);
            ensure!(step < 2, "Map fixture mutation count exceeded");
            match &self.mutation {
                Mutation::Layer(id) => {
                    let layer = self.layer(id).await?;
                    let title = format!("Source conformance layer {id} step {step}");
                    let result: FeatureLayer = tools::call(
                        &self.peer,
                        tool("update_feature_layer")?,
                        &UpdateFeatureLayerRequest {
                            layer_id: id.clone(),
                            expected_layer_revision: layer.revision,
                            title: Some(title.clone()),
                            description: None,
                            property_schema: None,
                            style: None,
                        },
                    )
                    .await?;
                    ensure!(
                        &result.layer_id == id && result.title == title,
                        "layer update receipt disagrees"
                    );
                }
                Mutation::Feature { layer, feature } => {
                    self.update_feature(layer, feature, step).await?
                }
            }
            Ok(())
        })
    }
    fn restart(&self) -> KnowledgeProbeFuture<'_> {
        Box::pin(self.restart.restart())
    }
}

pub fn tool(local: &str) -> Result<GatewayToolName> {
    Ok(GatewayToolName::from_parts(
        &"map".parse()?,
        &local.parse()?,
    )?)
}

pub async fn fixture_layer(peer: &Peer<RoleClient>, id: &FeatureLayerId) -> Result<FeatureLayer> {
    let layer: FeatureLayer = installed::read(
        peer,
        &MapKnowledgeMember::Layer { layer: id.clone() }.source_uri(),
    )
    .await?;
    ensure!(
        &layer.layer_id == id && layer.title.starts_with("Source conformance "),
        "Map mutation requires a selected source-conformance layer"
    );
    Ok(layer)
}
