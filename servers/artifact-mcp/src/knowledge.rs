//! Artifact owns its metadata collection and maps service snapshots to observations.
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use veoveo_artifact_contract::ArtifactMetadataSnapshot;
use veoveo_mcp_knowledge_extension::{
    AccessDescriptor, AccessModel, ChangeSignal, CollectionDescriptor, Freshness, IndexingMode,
    Observation, ReadGrant, ReadPolicy, content_digest,
};

pub fn collection() -> CollectionDescriptor {
    CollectionDescriptor::new(
        "artifact.metadata".parse().expect("Artifact collection"),
        "artifact-metadata".parse().expect("Artifact entity kind"),
        veoveo_types::ResourceTemplateUri::new(crate::contract::INDEX_TEMPLATE)
            .expect("Artifact index template"),
        Freshness::max_age(300),
        ChangeSignal::Listen,
        AccessModel::WorkContext,
        // The resource body is metadata. Artifact bytes are a separate resource.
        IndexingMode::Content,
    )
    .expect("Artifact metadata collection")
}

/// The service authorizes and assembles the snapshot. This function performs no I/O
/// and accepts no caller-supplied attribution or access policy.
pub fn metadata_document(
    snapshot: &ArtifactMetadataSnapshot,
    observed_at: DateTime<Utc>,
) -> Result<(String, Observation)> {
    let metadata = snapshot.metadata();
    let access = access_descriptor(snapshot)?;
    let text = serde_json::to_string(metadata)?;
    ensure!(
        text.len() <= 64 * 1024,
        "Artifact metadata member exceeds 64 KiB"
    );
    let revision = content_digest(&serde_json::to_string(&(&text, &access))?);
    let descriptor = collection();
    let observation = Observation::builder(
        descriptor.collection().clone(),
        revision.to_string().parse()?,
        content_digest(&text),
        observed_at,
    )
    .access(access)
    .modified_at(snapshot.metadata_updated_at())
    // Artifact records no principal responsible for the latest metadata update.
    .build(&descriptor)?;
    Ok((text, observation))
}

/// Artifact-owned read policy shared by resources derived from an occurrence.
/// It describes access; the Artifact service still authorizes every source read.
pub fn access_descriptor(snapshot: &ArtifactMetadataSnapshot) -> Result<AccessDescriptor> {
    let metadata = snapshot.metadata();
    let compliance = &metadata.compliance;
    let owner = compliance.owner.clone().expect("checked snapshot owner");
    // Artifact's protected owner grant confers read until occurrence retention.
    // Reject corrupt source state instead of inventing implicit owner access.
    ensure!(
        snapshot
            .read_grants()
            .iter()
            .any(|grant| grant.subject == owner
                && grant.expires_at == compliance.retention_expires_at),
        "Artifact snapshot has no protected owner read grant"
    );
    let mut labels = compliance.data_labels.clone();
    labels.extend(compliance.classification.iter().cloned());
    Ok(AccessDescriptor {
        tenant: compliance
            .tenant_id
            .clone()
            .expect("checked snapshot tenant"),
        work_context: compliance
            .work_context
            .clone()
            .expect("checked snapshot context"),
        read_policy: ReadPolicy::SelectedWorkContext {},
        grants: snapshot
            .read_grants()
            .iter()
            .filter(|grant| grant.subject != owner)
            .map(|grant| ReadGrant {
                subject: grant.subject.clone(),
                expires_at: grant.expires_at,
            })
            .collect(),
        owner,
        data_labels: labels.into_iter().collect(),
        expires_at: compliance.retention_expires_at,
    })
}

#[cfg(test)]
mod tests;
