use super::*;
use serde_json::json;
use veoveo_artifact_contract::{ArtifactId, ArtifactMetadata, Grant};
use veoveo_types::{AccessLevel, AccessSubject};

fn fixture() -> (ArtifactMetadata, Vec<Grant>, DateTime<Utc>) {
    let id = ArtifactId::new();
    let time = "2026-10-01T00:00:00Z".parse().unwrap();
    let metadata = serde_json::from_value(json!({
        "artifact_id":id, "artifact_uri":id.plane_uri(), "byte_len":12,
        "filename":"inspection.json", "mime_type":"application/json", "created_at":time,
        "compliance":{
            "tenant_id":"acme", "work_context":"mission", "owner":{"kind":"principal","id":"author"},
            "classification":"secret", "data_labels":["mission-data"],
            "provenance":{"producer":"author","invocation_mode":"direct","initiator":"author","policy_revision":"r1"}
        }, "metadata":{"purpose":"inspection"}
    })).unwrap();
    let grant = Grant {
        artifact: id,
        subject: AccessSubject::Principal("author".parse().unwrap()),
        level: AccessLevel::Admin,
        tenant: "acme".parse().unwrap(),
        data_labels: Default::default(),
        retention_expires_at: None,
    };
    (metadata, vec![grant], time)
}

#[test]
fn metadata_observation_preserves_source_policy_and_revises_on_access_changes() {
    let (metadata, mut grants, time) = fixture();
    let snapshot = ArtifactMetadataSnapshot::new(metadata.clone(), grants.clone(), time).unwrap();
    let (text, original) = metadata_document(&snapshot, time).unwrap();
    assert_eq!(original.content_sha256(), &content_digest(&text));
    assert_eq!(
        serde_json::from_str::<ArtifactMetadata>(&text).unwrap(),
        metadata
    );
    assert_eq!(original.modified_at(), Some(time));
    assert!(original.modified_by().is_none());
    let access = original.access().unwrap();
    assert_eq!(access.read_policy, ReadPolicy::SelectedWorkContext {});
    assert_eq!(access.work_context.as_str(), "mission");
    assert_eq!(access.tenant.as_str(), "acme");
    assert_eq!(
        access
            .data_labels
            .iter()
            .map(|label| label.as_str())
            .collect::<Vec<_>>(),
        ["mission-data", "secret"]
    );
    let deadline = time + chrono::TimeDelta::hours(1);
    let mut read = grants[0].clone();
    read.subject = AccessSubject::Group("reviewers".parse().unwrap());
    read.level = AccessLevel::Read;
    read.retention_expires_at = Some(deadline);
    grants.push(read);
    let shared = ArtifactMetadataSnapshot::new(metadata.clone(), grants.clone(), time).unwrap();
    let (same_text, shared_observation) = metadata_document(&shared, time).unwrap();
    assert_eq!(text, same_text);
    assert_ne!(original.revision(), shared_observation.revision());
    assert_eq!(
        shared_observation.access().unwrap().grants[0].expires_at,
        Some(deadline)
    );
    grants.reverse();
    let reordered = ArtifactMetadataSnapshot::new(metadata.clone(), grants.clone(), time).unwrap();
    assert_eq!(
        metadata_document(&reordered, time).unwrap().1.revision(),
        shared_observation.revision()
    );
    let mut expiring = metadata.clone();
    expiring.compliance.retention_expires_at = Some(deadline);
    grants
        .iter_mut()
        .for_each(|grant| grant.retention_expires_at = Some(deadline));
    let expiring = ArtifactMetadataSnapshot::new(expiring, grants, time).unwrap();
    let observation = metadata_document(&expiring, time).unwrap().1;
    assert_eq!(observation.access().unwrap().expires_at, Some(deadline));
    assert_ne!(observation.revision(), shared_observation.revision());
    let invalid = ArtifactMetadataSnapshot::new(metadata, vec![], time).unwrap();
    assert!(metadata_document(&invalid, time).is_err());
}

#[cfg(feature = "mcp")]
#[test]
fn artifact_members_negotiate_conditions_after_snapshot_admission() {
    use veoveo_mcp_knowledge_extension::{client, server};
    let (mut metadata, grants, time) = fixture();
    let uri = crate::contract::ArtifactResource::Metadata(metadata.artifact_id()).to_uri();
    let snapshot = ArtifactMetadataSnapshot::new(metadata.clone(), grants.clone(), time).unwrap();
    let (text, observation) = metadata_document(&snapshot, time).unwrap();
    let mut request = rmcp::model::RequestMetaObject::default();
    let plain = server::member_result(
        &uri,
        "application/json",
        text.clone(),
        observation.clone(),
        &collection(),
        None,
    )
    .unwrap();
    assert!(plain.meta.is_none());
    assert_eq!(plain.contents.len(), 1);
    client::declare_read(&mut request, None);
    let full = server::member_result(
        &uri,
        "application/json",
        text.clone(),
        observation.clone(),
        &collection(),
        Some(&request),
    )
    .unwrap();
    assert_eq!(
        client::validate_read(&full, &uri, None).unwrap(),
        Some(observation.clone())
    );
    client::declare_read(&mut request, Some(observation.revision()));
    let unchanged = server::member_result(
        &uri,
        "application/json",
        text,
        observation.clone(),
        &collection(),
        Some(&request),
    )
    .unwrap();
    assert!(unchanged.contents.is_empty());
    assert!(
        client::validate_read(&unchanged, &uri, Some(observation.revision()))
            .unwrap()
            .unwrap()
            .not_modified()
    );
    metadata.release_state = veoveo_artifact_contract::ArtifactReleaseState::Releasable;
    let snapshot =
        ArtifactMetadataSnapshot::new(metadata, grants, time + chrono::TimeDelta::seconds(1))
            .unwrap();
    let (text, changed) =
        metadata_document(&snapshot, time + chrono::TimeDelta::seconds(1)).unwrap();
    assert_ne!(changed.revision(), observation.revision());
    let result = server::member_result(
        &uri,
        "application/json",
        text,
        changed,
        &collection(),
        Some(&request),
    )
    .unwrap();
    assert_eq!(result.contents.len(), 1);
    assert!(
        !client::validate_read(&result, &uri, Some(observation.revision()))
            .unwrap()
            .unwrap()
            .not_modified()
    );
}

#[test]
fn oversized_metadata_fails_before_becoming_a_member() {
    let (mut metadata, grants, time) = fixture();
    metadata.metadata = json!({"large": "x".repeat(64 * 1024)});
    let snapshot = ArtifactMetadataSnapshot::new(metadata, grants, time).unwrap();
    assert!(metadata_document(&snapshot, time).is_err());
}
