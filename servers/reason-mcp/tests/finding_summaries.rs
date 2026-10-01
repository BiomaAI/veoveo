//! Stored findings and negotiated observations; no inference is performed.
#![cfg(feature = "mcp")]
#[path = "support/finding.rs"]
mod fixture;
use chrono::{DateTime, Utc};
use serde_json::json;
use veoveo_artifact_contract::{ArtifactId, ArtifactMetadata, ArtifactMetadataSnapshot, Grant};
use veoveo_mcp_knowledge_extension::{client, server};
use veoveo_reason_mcp::{
    contract::*,
    knowledge::{AdmittedFinding, FindingPosition, summary},
};
use veoveo_types::{AccessLevel, AccessSubject, ResourceAddress};

fn source() -> (
    AdmittedFinding,
    ArtifactMetadata,
    Vec<Grant>,
    ReasoningResults,
    DateTime<Utc>,
) {
    let time = "2026-10-01T00:00:00Z".parse().unwrap();
    let results = fixture::results();
    let analysis = "01983da0-0000-7000-8000-000000000010".parse().unwrap();
    let artifact = ArtifactId::new();
    let metadata = serde_json::from_value(json!({
        "artifact_id":artifact, "artifact_uri":artifact.plane_uri(), "byte_len":4096, "created_at":time,
        "compliance":{"tenant_id":"acme", "work_context":"operations", "owner":{"kind":"principal","id":"author"},
            "data_labels":["traffic"], "provenance":{"producer":"worker", "invocation_mode":"automated", "policy_revision":"r1"}},
        "metadata": ReasonArtifactMetadata { provenance: ReasonArtifactProvenance::Results {
            analysis_id:analysis, recording_id:results.recording_uri.id(), pipeline_id:results.pipeline_id.clone(),
            model_id:results.model_id.clone(), prompt_revision:results.prompt_revision.clone(), task_kind:(&results.task).into(),
            source_snapshot_sha256:results.source_snapshot.digest_sha256().unwrap(),
        }}
    })).unwrap();
    let grants = vec![Grant {
        artifact,
        subject: AccessSubject::Principal("author".parse().unwrap()),
        level: AccessLevel::Admin,
        tenant: "acme".parse().unwrap(),
        data_labels: Default::default(),
        retention_expires_at: None,
    }];
    (
        AdmittedFinding {
            position: FindingPosition {
                created_at: time,
                analysis,
            },
            updated_at: time,
            pipeline: results.pipeline_id.clone(),
            model: results.model_id.clone(),
            results: artifact,
            expires_at: None,
        },
        metadata,
        grants,
        results,
        time,
    )
}

#[test]
fn findings_negotiate_conditions_and_access_changes_without_changing_content() {
    let (mut finding, metadata, mut grants, results, time) = source();
    for kind in FindingCollection::ALL {
        let snapshot =
            ArtifactMetadataSnapshot::new(metadata.clone(), grants.clone(), time).unwrap();
        let (text, observation) =
            summary::summarize(kind, &finding, &snapshot, &results, time).unwrap();
        let uri = FindingResource::Member {
            collection: kind,
            analysis: finding.position.analysis,
        }
        .to_uri()
        .unwrap();
        let descriptor = summary::collection(kind);
        let ordinary = server::member_result(
            &uri,
            "application/json",
            text.clone(),
            observation.clone(),
            &descriptor,
            None,
        )
        .unwrap();
        assert!(ordinary.meta.is_none());
        let mut request = rmcp::model::RequestMetaObject::default();
        client::declare_read(&mut request, None);
        let full = server::member_result(
            &uri,
            "application/json",
            text.clone(),
            observation.clone(),
            &descriptor,
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
            text.clone(),
            observation.clone(),
            &descriptor,
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
        let mut grant = grants[0].clone();
        grant.subject = AccessSubject::Principal("reader".parse().unwrap());
        grant.level = AccessLevel::Read;
        grant.retention_expires_at = Some(time + chrono::TimeDelta::hours(1));
        grants.push(grant);
        let shared = ArtifactMetadataSnapshot::new(metadata.clone(), grants.clone(), time).unwrap();
        let (same, changed) = summary::summarize(kind, &finding, &shared, &results, time).unwrap();
        assert_eq!(same, text);
        assert_eq!(changed.content_sha256(), observation.content_sha256());
        assert_ne!(changed.revision(), observation.revision());
        let changed_result = server::member_result(
            &uri,
            "application/json",
            same,
            changed,
            &descriptor,
            Some(&request),
        )
        .unwrap();
        assert_eq!(changed_result.contents.len(), 1);
        grants.pop();
        finding.expires_at = Some(time + chrono::TimeDelta::minutes(20));
        let (_, retained) = summary::summarize(kind, &finding, &snapshot, &results, time).unwrap();
        assert_eq!(retained.access().unwrap().expires_at, finding.expires_at);
        assert_ne!(retained.revision(), observation.revision());
        assert!(
            summary::summarize(
                kind,
                &finding,
                &snapshot,
                &results,
                finding.expires_at.unwrap()
            )
            .is_err()
        );
        finding.expires_at = None;
    }
}

#[test]
fn stored_results_must_agree_with_task_artifact_and_provenance() {
    let (finding, metadata, grants, results, time) = source();
    for mutation in [
        "analysis_id",
        "model_id",
        "prompt_revision",
        "source_snapshot_sha256",
    ] {
        let mut corrupted = metadata.clone();
        corrupted.metadata["provenance"][mutation] = match mutation {
            "analysis_id" => "01983da0-0000-7000-8000-000000000099".into(),
            "source_snapshot_sha256" => "b".repeat(64).into(),
            _ => "different".into(),
        };
        let snapshot = ArtifactMetadataSnapshot::new(corrupted, grants.clone(), time).unwrap();
        assert!(
            summary::summarize(
                FindingCollection::Results,
                &finding,
                &snapshot,
                &results,
                time
            )
            .is_err(),
            "{mutation}"
        );
    }
    let snapshot = ArtifactMetadataSnapshot::new(metadata, grants, time).unwrap();
    let mut wrong = finding.clone();
    wrong.results = ArtifactId::new();
    assert!(
        summary::summarize(
            FindingCollection::Results,
            &wrong,
            &snapshot,
            &results,
            time
        )
        .is_err()
    );
    let mut wrong = results;
    wrong.schema = "unsupported".into();
    assert!(
        summary::summarize(
            FindingCollection::Results,
            &finding,
            &snapshot,
            &wrong,
            time
        )
        .is_err()
    );
}
