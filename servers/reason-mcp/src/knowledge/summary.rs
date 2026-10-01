//! Source bytes, recorded provenance and Artifact-owned access form one finding.
use super::AdmittedFinding;
use crate::contract::*;
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use veoveo_artifact_contract::ArtifactMetadataSnapshot;
use veoveo_mcp_knowledge_extension::{
    AccessModel, ChangeSignal, CollectionDescriptor, Freshness, IndexingMode, Observation,
    content_digest,
};
use veoveo_types::ResourceTemplateUri;

pub fn collection(kind: FindingCollection) -> CollectionDescriptor {
    let (id, entity) = match kind {
        FindingCollection::Analyses => ("reason.analyses", "completed-analysis"),
        FindingCollection::Results => ("reason.results", "reasoning-result"),
    };
    CollectionDescriptor::new(
        id.parse().expect("Reason collection"),
        entity.parse().expect("Reason entity"),
        ResourceTemplateUri::new(kind.page_template()).expect("Reason collection template"),
        Freshness::max_age(300),
        ChangeSignal::Listen,
        AccessModel::WorkContext,
        IndexingMode::Content,
    )
    .expect("Reason finding descriptor")
}

/// The caller supplies a SQL-admitted finding and a service-authorized snapshot.
/// This pure conversion never obtains authority from model output or metadata.
pub fn summarize(
    kind: FindingCollection,
    finding: &AdmittedFinding,
    snapshot: &ArtifactMetadataSnapshot,
    results: &ReasoningResults,
    observed_at: DateTime<Utc>,
) -> Result<(String, Observation)> {
    ensure!(
        snapshot.metadata().artifact_id() == finding.results,
        "finding Artifact changed"
    );
    ensure!(
        results.schema == crate::executor::REASONING_RESULTS_SCHEMA,
        "unsupported Reason result schema"
    );
    ensure!(
        results.pipeline_id == finding.pipeline && results.model_id == finding.model,
        "finding result disagrees with Task output"
    );
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
            && recording_id == results.recording_uri.id()
            && pipeline_id == results.pipeline_id
            && model_id == results.model_id
            && prompt_revision == results.prompt_revision
            && task_kind == ReasoningKind::from(&results.task)
            && source_snapshot_sha256 == results.source_snapshot.digest_sha256()?,
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
        results,
    )?)?;
    let descriptor = collection(kind);
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
