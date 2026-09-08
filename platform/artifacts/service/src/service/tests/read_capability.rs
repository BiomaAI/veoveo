use super::*;
use crate::ledger::ReadContextVersion;
use std::num::NonZeroU32;
use veoveo_mcp_contract::{
    ArtifactReadCapabilityId, ArtifactTaskId, IssueArtifactReadCapabilityRequest,
    IssuedArtifactReadCapability,
};

mod native;

fn admit_context(repository: &InMemoryRepository, caller: &PlaneCaller) {
    repository.set_read_context(
        caller.tenant().unwrap().clone(),
        caller.identity.authority.work_context.clone(),
        ReadContextVersion {
            policy_revision: caller.identity.authority.policy_revision.clone(),
            digest: "a".repeat(64),
        },
    );
}

fn request(count: u32, bytes: u64) -> IssueArtifactReadCapabilityRequest {
    IssueArtifactReadCapabilityRequest {
        task_id: ArtifactTaskId::new(),
        expires_at: Utc::now() + TimeDelta::hours(1),
        max_artifact_count: NonZeroU32::new(count).unwrap(),
        max_total_bytes: NonZeroU64::new(bytes).unwrap(),
    }
}

async fn read<R: ArtifactRepository, S: BlobStore>(
    service: &ArtifactService<R, S>,
    cap: &IssuedArtifactReadCapability,
    artifact: ArtifactId,
) -> Result<ArtifactMetadata, ArtifactPlaneError> {
    service
        .head_with_read_capability(
            cap.capability_id,
            cap.secret.expose_secret(),
            cap.task_id,
            artifact,
        )
        .await
}

#[tokio::test]
async fn artifact_read_delegation_rechecks_grants_clearance_tenant_and_revocation() {
    let (service, repository) = service();
    let alice = caller("alice", "acme", &["private"]);
    let mut bob = caller("bob", "acme", &[]);
    bob.identity.authority.work_context = WorkContextId::new("research").unwrap();
    admit_context(&repository, &bob);
    let artifact = service
        .put(&alice, PutArtifactRequest::default(), b"recording".to_vec())
        .await
        .unwrap();
    service
        .grant(
            &alice,
            &artifact.artifact_id,
            AccessSubject::Principal(bob.identity.actor.id.clone()),
            AccessLevel::Read,
        )
        .await
        .unwrap();
    let cap = service
        .issue_read_capability(&bob, request(10, 1024))
        .await
        .unwrap();
    read(&service, &cap, artifact.artifact_id).await.unwrap();
    service
        .revoke(
            &alice,
            &artifact.artifact_id,
            &AccessSubject::Principal(bob.identity.actor.id.clone()),
        )
        .await
        .unwrap();
    assert!(
        read(&service, &cap, artifact.artifact_id).await.is_err(),
        "cached admission bypassed grant revocation"
    );
    service
        .grant(
            &alice,
            &artifact.artifact_id,
            AccessSubject::Principal(bob.identity.actor.id.clone()),
            AccessLevel::Read,
        )
        .await
        .unwrap();
    read(&service, &cap, artifact.artifact_id).await.unwrap();
    let sensitive = service
        .put(
            &alice,
            PutArtifactRequest {
                classification: Some(DataLabelId::new("private").unwrap()),
                ..Default::default()
            },
            b"sensitive".to_vec(),
        )
        .await
        .unwrap();
    service
        .grant(
            &alice,
            &sensitive.artifact_id,
            AccessSubject::Principal(bob.identity.actor.id.clone()),
            AccessLevel::Read,
        )
        .await
        .unwrap();
    assert!(read(&service, &cap, sensitive.artifact_id).await.is_err());
    let foreign = service
        .put(
            &caller("bob", "other-tenant", &[]),
            PutArtifactRequest::default(),
            b"foreign".to_vec(),
        )
        .await
        .unwrap();
    assert!(read(&service, &cap, foreign.artifact_id).await.is_err());
    assert!(
        service
            .revoke_read_capability(&alice, cap.capability_id)
            .await
            .is_err()
    );
    service
        .revoke_read_capability(&bob, cap.capability_id)
        .await
        .unwrap();
    assert!(read(&service, &cap, artifact.artifact_id).await.is_err());
}

#[tokio::test]
async fn artifact_read_delegation_binds_task_context_and_secret_and_never_reuses_expired_admission()
{
    let (service, repository) = service();
    let alice = caller("alice", "acme", &[]);
    admit_context(&repository, &alice);
    let artifact = service
        .put(&alice, PutArtifactRequest::default(), b"recording".to_vec())
        .await
        .unwrap();
    let cap = service
        .issue_read_capability(&alice, request(2, 100))
        .await
        .unwrap();
    assert!(
        service
            .head_with_read_capability(
                cap.capability_id,
                cap.secret.expose_secret(),
                ArtifactTaskId::new(),
                artifact.artifact_id
            )
            .await
            .is_err()
    );
    assert!(
        service
            .head_with_read_capability(
                cap.capability_id,
                &"wrong-secret".repeat(4),
                cap.task_id,
                artifact.artifact_id
            )
            .await
            .is_err()
    );
    assert!(
        service
            .head_with_read_capability(
                ArtifactReadCapabilityId::new(),
                cap.secret.expose_secret(),
                cap.task_id,
                artifact.artifact_id
            )
            .await
            .is_err()
    );
    read(&service, &cap, artifact.artifact_id).await.unwrap();
    repository.set_read_context(
        alice.tenant().unwrap().clone(),
        alice.identity.authority.work_context.clone(),
        ReadContextVersion {
            policy_revision: alice.identity.authority.policy_revision.clone(),
            digest: "b".repeat(64),
        },
    );
    assert!(read(&service, &cap, artifact.artifact_id).await.is_err());
    admit_context(&repository, &alice);
    let mut expiring = request(2, 100);
    expiring.expires_at = Utc::now() + TimeDelta::seconds(1);
    let expiring = service
        .issue_read_capability(&alice, expiring)
        .await
        .unwrap();
    read(&service, &expiring, artifact.artifact_id)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    assert!(
        read(&service, &expiring, artifact.artifact_id)
            .await
            .is_err()
    );
    let mut expired = request(2, 100);
    expired.expires_at = Utc::now() - TimeDelta::seconds(1);
    assert!(
        service
            .issue_read_capability(&alice, expired)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn artifact_read_delegation_reserves_distinct_bytes_once_and_serializes_quota() {
    let (service, repository) = service();
    let alice = caller("alice", "acme", &[]);
    admit_context(&repository, &alice);
    let first = service
        .put(&alice, PutArtifactRequest::default(), vec![1; 3])
        .await
        .unwrap()
        .artifact_id;
    let second = service
        .put(&alice, PutArtifactRequest::default(), vec![2; 3])
        .await
        .unwrap()
        .artifact_id;
    let cap = service
        .issue_read_capability(&alice, request(2, 5))
        .await
        .unwrap();
    let (one, two) = tokio::join!(read(&service, &cap, first), read(&service, &cap, second));
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    let winner = if one.is_ok() { first } else { second };
    read(&service, &cap, winner).await.unwrap();
    read(&service, &cap, winner).await.unwrap();
    let count_cap = service
        .issue_read_capability(&alice, request(1, 100))
        .await
        .unwrap();
    read(&service, &count_cap, first).await.unwrap();
    assert!(read(&service, &count_cap, second).await.is_err());
}
