//! A disposable dataset changes active releases and restores its initial pointer.
use crate::{authoring::tool, installed, restart::DeploymentRestart, tools};
use anyhow::{Result, ensure};
use rmcp::{Peer, RoleClient};
use serde::Deserialize;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use veoveo_map_mcp::contract::*;
use veoveo_mcp_conformance::knowledge_probes::{KnowledgeChangeDriver, KnowledgeProbeFuture};
use veoveo_types::ResourceAddress;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Releases {
    pub dataset: MapDatasetId,
    pub original: DatasetReleaseId,
    pub candidate: DatasetReleaseId,
}

pub struct ReleaseProbe {
    pub selection: Releases,
    peer: Peer<RoleClient>,
    restart: Arc<DeploymentRestart>,
    changes: AtomicUsize,
}
impl ReleaseProbe {
    pub async fn new(
        selection: Releases,
        peer: Peer<RoleClient>,
        restart: Arc<DeploymentRestart>,
    ) -> Result<Self> {
        let probe = Self {
            selection,
            peer,
            restart,
            changes: AtomicUsize::new(0),
        };
        ensure!(
            probe.selection.original != probe.selection.candidate,
            "release fixtures must differ"
        );
        let original = probe.release(&probe.selection.original).await?;
        let candidate = probe.release(&probe.selection.candidate).await?;
        ensure!(
            original.state == DatasetReleaseState::Active
                && candidate.state == DatasetReleaseState::Staged
                && original.source_id == candidate.source_id,
            "release fixture requires an active/staged pair from one source"
        );
        ensure!(
            probe.active().await?.release_id == probe.selection.original,
            "fixture pointer does not select original release"
        );
        Ok(probe)
    }

    pub async fn release(&self, id: &DatasetReleaseId) -> Result<DatasetRelease> {
        let value: DatasetRelease = installed::read(
            &self.peer,
            &MapKnowledgeMember::Release {
                dataset: self.selection.dataset.clone(),
                release: id.clone(),
            }
            .source_uri(),
        )
        .await?;
        ensure!(
            &value.release_id == id
                && value.dataset_id == self.selection.dataset
                && value.routing_build_version.is_none(),
            "release mutation requires a disposable non-routing fixture dataset"
        );
        let source: SourceSummary = installed::read(
            &self.peer,
            &MapSourceUri::new(value.source_id.clone()).to_uri()?,
        )
        .await?;
        ensure!(
            source.source_id() == &value.source_id
                && source.dataset_id() == &self.selection.dataset
                && source.name().starts_with("Source conformance ")
                && source.authority() == AuthorityClass::SyntheticTest
                && source.adapter_kind() == SourceAdapterKind::AuthorityVector,
            "release mutation requires a selected synthetic source-conformance vector source"
        );
        value.validate()?;
        Ok(value)
    }

    async fn active(&self) -> Result<ActiveReleasePointer> {
        let result: ListActiveDatasetReleasesOutput = tools::call(
            &self.peer,
            tool("list_active_dataset_releases")?,
            &ListActiveDatasetReleasesRequest {
                source_id: None,
                dataset_id: Some(self.selection.dataset.clone()),
                limit: 2,
            },
        )
        .await?;
        ensure!(
            !result.truncated && result.releases.len() == 1,
            "fixture dataset needs one active release"
        );
        let pointer = result.releases.into_iter().next().unwrap().pointer;
        ensure!(
            pointer.dataset_id == self.selection.dataset
                && [
                    self.selection.original.clone(),
                    self.selection.candidate.clone()
                ]
                .contains(&pointer.release_id),
            "fixture pointer changed outside this test; refusing to overwrite it"
        );
        Ok(pointer)
    }

    async fn select(&self, id: &DatasetReleaseId, rollback: bool) -> Result<()> {
        let pointer = self.active().await?;
        let release = self.release(id).await?;
        let result: ReleaseMutationResponse = tools::call(
            &self.peer,
            tool(if rollback {
                "rollback_release"
            } else {
                "activate_release"
            })?,
            &ReleaseMutationRequest {
                release_id: id.clone(),
                expected_record_version: release.record_version,
                expected_active_pointer_version: pointer.record_version,
            },
        )
        .await?;
        ensure!(
            &result.release.release_id == id && result.release.state == DatasetReleaseState::Active,
            "release activation receipt disagrees"
        );
        ensure!(
            &self.active().await?.release_id == id,
            "release activation did not move the fixture pointer"
        );
        Ok(())
    }

    pub async fn cleanup(&self) -> Result<()> {
        if self.active().await?.release_id != self.selection.original {
            self.select(&self.selection.original, true).await?;
        }
        ensure!(
            self.active().await?.release_id == self.selection.original,
            "fixture active release was not restored"
        );
        Ok(())
    }
}
impl KnowledgeChangeDriver for ReleaseProbe {
    fn mutate(&self) -> KnowledgeProbeFuture<'_> {
        Box::pin(async move {
            match self.changes.fetch_add(1, Ordering::SeqCst) {
                0 => self.select(&self.selection.candidate, false).await,
                1 => self.select(&self.selection.original, true).await,
                _ => anyhow::bail!("release fixture mutation count exceeded"),
            }
        })
    }
    fn restart(&self) -> KnowledgeProbeFuture<'_> {
        Box::pin(self.restart.restart())
    }
}
