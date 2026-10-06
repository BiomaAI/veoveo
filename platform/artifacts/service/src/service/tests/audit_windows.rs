use super::*;
use veoveo_mcp_contract::audit::{ArtifactActivity, AuditDetail, AuditOutcome};

#[tokio::test]
async fn access_request_and_capability_denials_keep_requested_targets_and_caller_attribution() {
    let (service, repository) = service();
    let owner = caller("owner", "acme", &["controlled"]);
    let artifact = service
        .put(
            &owner,
            PutArtifactRequest {
                classification: Some(DataLabelId::parse("controlled").unwrap()),
                ..PutArtifactRequest::default()
            },
            vec![1; 8],
        )
        .await
        .unwrap()
        .artifact_id();
    for (tenant, reason) in [
        ("other", AuditReason::TenantMismatch),
        ("acme", AuditReason::InsufficientClearance),
    ] {
        let denied = caller("denied", tenant, &[]);
        assert!(
            service
                .create_access_request(
                    &denied,
                    &artifact,
                    CreateArtifactAccessRequest {
                        requested_level: AccessLevel::Read,
                        justification: "Fixture request".into(),
                    }
                )
                .await
                .is_err()
        );
        let records = repository.audit_records();
        let record = records.last().unwrap();
        assert_eq!(record.outcome(), AuditOutcome::Denied);
        assert_eq!(record.reason(), reason);
        assert_eq!(
            record.target(),
            &veoveo_mcp_contract::audit::AuditTarget::Artifact { artifact }
        );
        assert_eq!(
            record.actor().unwrap().tenant.as_ref().unwrap().as_str(),
            tenant
        );
    }
    let mut viewer = caller("viewer", "acme", &[]);
    viewer.identity.authority.membership = WorkContextMembershipLevel::Viewer;
    assert!(
        service
            .list_access_requests(
                &viewer,
                ListArtifactAccessRequests {
                    scope: Some(ArtifactAccessRequestScope::Reviewable),
                    ..ListArtifactAccessRequests::default()
                }
            )
            .await
            .is_err()
    );
    assert_eq!(
        repository.audit_records().last().unwrap().reason(),
        AuditReason::InsufficientAccess
    );
    let capability = veoveo_artifact_contract::ArtifactReadCapabilityId::new();
    assert!(
        service
            .head_with_read_capability(
                capability,
                "invalid",
                veoveo_artifact_contract::ArtifactTaskId::new(),
                artifact
            )
            .await
            .is_err()
    );
    let records = repository.audit_records();
    let record = records.last().unwrap();
    assert!(
        record.actor().is_none(),
        "an invalid credential cannot claim its issuing actor"
    );
    assert_eq!(record.reason(), AuditReason::InvalidCredential);
    assert_eq!(
        record.target(),
        &veoveo_mcp_contract::audit::AuditTarget::Artifact { artifact }
    );
    repository.reject_required_audit(true);
    assert!(matches!(
        service
            .list_access_requests(
                &viewer,
                ListArtifactAccessRequests {
                    scope: Some(ArtifactAccessRequestScope::Reviewable),
                    ..ListArtifactAccessRequests::default()
                }
            )
            .await,
        Err(ArtifactPlaneError::Transport(_))
    ));
}

#[tokio::test]
async fn concurrent_ranges_share_one_window_and_every_denial_is_recorded() {
    let (service, repository) = service();
    let owner = caller("owner", "acme", &[]);
    let mut denied = caller("other", "acme", &[]);
    denied.identity.authority.work_context = WorkContextId::parse("other-work").unwrap();
    bind_request_context(&mut denied.identity);
    let artifact = service
        .put(&owner, PutArtifactRequest::default(), vec![1; 32])
        .await
        .unwrap();
    let before = Utc::now().timestamp().div_euclid(300);
    let results = futures::future::join_all((0..100).map(|_| {
        service.download(
            &owner,
            artifact.artifact_id(),
            Some(ArtifactByteRange::Inclusive { start: 0, end: 7 }),
            DownloadBody::Omit,
        )
    }))
    .await;
    assert!(results.into_iter().all(|result| result.is_ok()));
    let after = Utc::now().timestamp().div_euclid(300);
    for _ in 0..3 {
        assert!(matches!(
            service
                .download(&denied, artifact.artifact_id(), None, DownloadBody::Omit)
                .await,
            Err(ArtifactPlaneError::Denied(AccessDecision::DenyNeedToKnow))
        ));
    }
    let records = repository.audit_records();
    let downloads: Vec<_> = records
        .iter()
        .filter(|record| {
            matches!(
                record.detail(),
                AuditDetail::Artifact {
                    activity: ArtifactActivity::Download,
                    ..
                }
            )
        })
        .collect();
    let allowed = downloads
        .iter()
        .filter(|record| record.outcome() == AuditOutcome::Allowed)
        .count();
    assert!(allowed >= 1 && allowed as i64 <= after - before + 1);
    assert_eq!(
        downloads
            .iter()
            .filter(|record| record.outcome() == AuditOutcome::Denied)
            .count(),
        3
    );
    assert!(
        downloads
            .iter()
            .all(|record| record.actor().unwrap().tenant.as_ref().unwrap().as_str() == "acme")
    );
}

#[tokio::test]
async fn failed_first_window_commit_blocks_delivery_and_can_be_retried() {
    let (service, repository) = service();
    let owner = caller("owner", "acme", &[]);
    let artifact = service
        .put(&owner, PutArtifactRequest::default(), vec![1; 32])
        .await
        .unwrap();
    repository.reject_required_audit(true);
    assert!(matches!(
        service
            .download(&owner, artifact.artifact_id(), None, DownloadBody::Include)
            .await,
        Err(ArtifactPlaneError::Transport(_))
    ));
    repository.reject_required_audit(false);
    assert!(
        service
            .download(&owner, artifact.artifact_id(), None, DownloadBody::Include)
            .await
            .is_ok()
    );
    assert_eq!(
        repository
            .audit_records()
            .iter()
            .filter(|record| matches!(
                record.detail(),
                AuditDetail::Artifact {
                    activity: ArtifactActivity::Download,
                    ..
                }
            ))
            .count(),
        1
    );
}
