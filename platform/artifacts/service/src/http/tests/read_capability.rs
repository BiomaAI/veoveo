use super::*;
use veoveo_mcp_contract::{
    ArtifactReadAuthority, ArtifactTaskId, IssueArtifactReadCapabilityRequest,
};

#[tokio::test]
async fn task_read_http_authority_is_read_only_bound_revocable_and_streamed() {
    let (base, caller) = spawn_service().await;
    let plane = HttpArtifactPlane::new(&base);
    let metadata = plane
        .put(
            &caller,
            PutArtifactRequest::default(),
            b"recording-layer".to_vec(),
        )
        .await
        .unwrap();
    let cap = plane
        .issue_read_capability(
            &caller,
            &IssueArtifactReadCapabilityRequest {
                task_id: ArtifactTaskId::new(),
                expires_at: Utc::now() + TimeDelta::hours(1),
                max_artifact_count: std::num::NonZeroU32::new(1).unwrap(),
                max_total_bytes: std::num::NonZeroU64::new(1024).unwrap(),
            },
        )
        .await
        .unwrap();
    let scope = plane
        .read_capability_scope(&cap, cap.task_id)
        .await
        .unwrap();
    assert_eq!(scope.principal_id, caller.identity.actor.id);
    assert_eq!(Some(scope.tenant), caller.identity.actor.tenant);
    assert_eq!(scope.data_labels, caller.identity.actor.data_labels);
    assert!(
        plane
            .read_capability_scope(&cap, ArtifactTaskId::new())
            .await
            .is_err()
    );
    let authority = ArtifactReadAuthority::Task {
        capability: &cap,
        task_id: cap.task_id,
    };
    let downloaded = plane
        .download_with_authority(authority, metadata.artifact_id)
        .await
        .unwrap();
    assert_eq!(downloaded.metadata, metadata);
    assert_eq!(
        &downloaded.response.bytes().await.unwrap()[..],
        b"recording-layer"
    );
    plane
        .read_metadata(authority, metadata.artifact_id)
        .await
        .unwrap();
    let wrong_task = ArtifactTaskId::new();
    assert!(
        plane
            .read_metadata(
                ArtifactReadAuthority::Task {
                    capability: &cap,
                    task_id: wrong_task
                },
                metadata.artifact_id
            )
            .await
            .is_err()
    );
    // Exercise the server check independently of the client's early rejection.
    let response = reqwest::Client::new()
        .get(format!(
            "{base}/artifact-read-capabilities/{}/artifacts/{}/meta",
            cap.capability_id, metadata.artifact_id
        ))
        .query(&[("task_id", wrong_task.to_string())])
        .bearer_auth(cap.secret.expose_secret())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    let mut impersonation = caller.clone();
    impersonation.bearer_token = cap.secret.expose_secret().to_owned();
    assert!(
        plane
            .head(&impersonation, &metadata.artifact_id)
            .await
            .is_err()
    );
    assert!(
        plane
            .put(&impersonation, PutArtifactRequest::default(), vec![1])
            .await
            .is_err()
    );
    assert!(
        plane
            .issue_read_capability(
                &impersonation,
                &IssueArtifactReadCapabilityRequest {
                    task_id: cap.task_id,
                    expires_at: cap.expires_at,
                    max_artifact_count: std::num::NonZeroU32::new(1).unwrap(),
                    max_total_bytes: std::num::NonZeroU64::new(1).unwrap()
                }
            )
            .await
            .is_err()
    );
    plane
        .revoke_read_capability(&caller, cap.capability_id)
        .await
        .unwrap();
    assert!(
        plane
            .read_metadata(authority, metadata.artifact_id)
            .await
            .is_err()
    );
    assert!(
        plane
            .download_with_authority(authority, metadata.artifact_id)
            .await
            .is_err()
    );
    assert!(
        plane
            .read_capability_scope(&cap, cap.task_id)
            .await
            .is_err()
    );
    assert!(!format!("{cap:?}").contains(cap.secret.expose_secret()));
}
