use super::*;
use veoveo_types::DelegationId;

async fn assert_published_attribution<R: ArtifactRepository>(
    writer: &ArtifactService<R, InMemoryBlobStore>,
    reader: &ArtifactService<R, InMemoryBlobStore>,
) {
    for invocation in [
        InvocationProvenance::Direct {
            initiator: PrincipalId::new("alice").unwrap(),
        },
        InvocationProvenance::Delegated {
            initiator: PrincipalId::new("operator").unwrap(),
            delegation_id: DelegationId::new("delegation/17").unwrap(),
        },
        InvocationProvenance::Automated,
    ] {
        let automated = matches!(invocation, InvocationProvenance::Automated);
        let mut actor = caller(if automated { "worker" } else { "alice" }, "acme", &[]);
        actor.identity.authority.provenance = invocation.clone();
        if automated {
            actor.identity.actor.kind = PrincipalKind::Service;
        }
        let published = writer
            .put(&actor, PutArtifactRequest::default(), b"artifact".to_vec())
            .await
            .unwrap();
        let expected = ArtifactProvenance::new(
            actor.identity.actor.id.clone(),
            invocation,
            actor.identity.authority.policy_revision.clone(),
        );
        assert_eq!(published.compliance.provenance, Some(expected.clone()));
        let loaded = reader
            .get(&actor, &published.artifact_id(), AccessLevel::Read)
            .await
            .unwrap();
        assert_eq!(loaded.bytes, b"artifact");
        assert_eq!(loaded.metadata.compliance.provenance, Some(expected));
        assert_eq!(
            serde_json::to_value(loaded.metadata.compliance).unwrap(),
            serde_json::to_value(published.compliance).unwrap()
        );
    }
}

#[tokio::test]
async fn publication_preserves_authenticated_provenance() {
    let (service, _) = service();
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        assert_published_attribution(&service, &service),
    )
    .await
    .expect("in-memory provenance qualification timed out");
}

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.3.0 process"]
async fn provenance_survives_independent_repository_reads() {
    let mut database = native_database::Database::start();
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let blobs = InMemoryBlobStore::default();
        let writer = ArtifactService::new(
            crate::SurrealArtifactRepository::new(database.connect().await),
            blobs.clone(),
        );
        let reader = ArtifactService::new(
            crate::SurrealArtifactRepository::new(database.connect().await),
            blobs,
        );
        assert_published_attribution(&writer, &reader).await;
    })
    .await
    .expect("native provenance qualification timed out");
    database.finish();
}
