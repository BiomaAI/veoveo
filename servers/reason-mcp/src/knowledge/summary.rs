//! Retained findings, recorded provenance and Artifact-owned access form one finding.
use super::AdmittedFinding;
use crate::contract::*;
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use veoveo_artifact_contract::ArtifactMetadataSnapshot;
use veoveo_mcp_knowledge_extension::{Observation, content_digest};

/// The caller supplies a SQL-admitted finding and a service-authorized snapshot.
/// This pure conversion never obtains authority from model output or metadata.
pub fn summarize(
    kind: FindingCollection,
    finding: &AdmittedFinding,
    snapshot: &ArtifactMetadataSnapshot,
    observed_at: DateTime<Utc>,
) -> Result<(String, Observation)> {
    ensure!(
        snapshot.metadata().artifact_id() == finding.results,
        "finding Artifact changed"
    );
    let data = &finding.data;
    let metadata: ReasonArtifactMetadata =
        serde_json::from_value(snapshot.metadata().metadata.clone())?;
    let ReasonArtifactProvenance::Results {
        analysis_id,
        recording_id,
        pipeline_id,
        model_id,
        prompt_revision,
        task_kind,
        source_snapshot_sha256,
    } = metadata.provenance
    else {
        anyhow::bail!("finding requires Reason result provenance");
    };
    ensure!(
        analysis_id == finding.position.analysis
            && recording_id == data.recording_uri().id()
            && &pipeline_id == data.pipeline_id()
            && &model_id == data.model_id()
            && prompt_revision == data.prompt_revision()
            && task_kind == ReasoningKind::from(data.task())
            && &source_snapshot_sha256 == data.source_snapshot_sha256(),
        "finding provenance disagrees with its result"
    );
    let mut access = veoveo_artifact_mcp::knowledge::access_descriptor(snapshot)?;
    access.expires_at = access
        .expires_at
        .into_iter()
        .chain(finding.expires_at)
        .min();
    ensure!(
        access
            .expires_at
            .is_none_or(|deadline| deadline > observed_at),
        "finding expired during its read"
    );
    let text = serde_json::to_string(&FindingSummary::new(
        kind,
        finding.position.analysis,
        finding.results,
        finding.position.created_at,
        finding.updated_at,
        data,
    )?)?;
    let descriptor = kind.descriptor();
    let revision = content_digest(&serde_json::to_string(&(&text, &access))?);
    let observation = Observation::builder(
        descriptor.collection().clone(),
        revision.to_string().parse()?,
        content_digest(&text),
        observed_at,
    )
    .access(access)
    .modified_at(finding.updated_at)
    .build(&descriptor)?;
    Ok((text, observation))
}
