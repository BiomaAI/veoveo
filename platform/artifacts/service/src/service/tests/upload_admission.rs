//! Native quota and admission races; no file-sized buffers are needed for reservation.

use super::native_database::{Database, context};
use super::*;
use veoveo_platform_store as platform;

pub(super) async fn install_profile(store: &platform::PlatformStore) {
    install_profile_policy(store, None).await;
}

pub(super) async fn install_profile_policy(
    store: &platform::PlatformStore,
    upload: Option<veoveo_mcp_contract::ArtifactUploadPolicy>,
) {
    let now = Utc::now();
    let revision = platform::RecordId::new("gateway_control_revision", "upload-fixture");
    let revision_content = platform::GatewayControlRevisionContent {
        revision_id: "upload-fixture".into(),
        sha256: "a".repeat(64),
        source: platform::GatewayControlRevisionSource::SeedFile,
        applied_at: now,
        applied_by: "fixture".into(),
        tenant: None,
        control_plane: platform::OpenObject::default(),
    };
    let mut document = std::collections::BTreeMap::from([
        ("id".into(), serde_json::json!("fixture")),
        ("policy_version".into(), serde_json::json!("r1")),
    ]);
    if let Some(policy) = upload {
        document.insert(
            "artifact_upload".into(),
            serde_json::to_value(policy).unwrap(),
        );
    }
    let profile = platform::GatewayControlObjectContent {
        revision: revision.clone(),
        tenant: None,
        object_kind: "profile".into(),
        object_id: "fixture".into(),
        document: platform::OpenObject::new(document),
    };
    let policy = platform::GatewayControlObjectContent {
        revision: revision.clone(),
        tenant: None,
        object_kind: "policy".into(),
        object_id: "r1".into(),
        document: platform::OpenObject::new(std::collections::BTreeMap::from([(
            "version".into(),
            serde_json::json!("r1"),
        )])),
    };
    let active = platform::GatewayControlActiveRecord {
        id: platform::RecordId::new("gateway_control_active", "current"),
        revision: revision.clone(),
        revision_id: "upload-fixture".into(),
        updated_at: now,
    };
    store.client().query("CREATE ONLY $revision CONTENT $content; CREATE gateway_control_object CONTENT $profile; CREATE gateway_control_object CONTENT $policy; CREATE ONLY $active.id CONTENT $active;")
        .bind(("revision", revision)).bind(("content", revision_content)).bind(("profile", profile))
        .bind(("policy", policy)).bind(("active", active)).await.unwrap().check().unwrap();
}

pub(super) async fn admission(
    store: &platform::PlatformStore,
    actor: &PlaneCaller,
    size: i64,
) -> platform::ArtifactUploadRecord {
    let identity = store
        .ensure_identity(
            actor.tenant().unwrap().as_str(),
            actor.identity.actor.id.as_str(),
            actor.identity.actor.issuer.as_str(),
            actor.identity.actor.subject.as_str(),
            platform::PrincipalKind::User,
        )
        .await
        .unwrap();
    let version = store
        .artifact_upload_authority_version(
            &identity.tenant_key,
            actor.identity.authority.work_context.as_str(),
            "fixture",
        )
        .await
        .unwrap()
        .unwrap();
    let now = Utc::now();
    let id = uuid::Uuid::now_v7();
    platform::ArtifactUploadRecord {
        id: platform::upload_record_id(id),
        tenant: identity.tenant_id.record_id(),
        tenant_key: identity.tenant_key.clone(),
        actor: identity.principal_id.record_id(),
        actor_key: identity.principal_key.clone(),
        actor_kind: platform::PrincipalKind::User,
        actor_issuer: actor.identity.actor.issuer.to_string(),
        actor_subject: actor.identity.actor.subject.to_string(),
        profile_key: "fixture".into(),
        work_context: platform::deterministic_work_context_id(
            &identity.tenant_key,
            actor.identity.authority.work_context.as_str(),
        )
        .unwrap()
        .record_id(),
        authority: platform::InvocationAuthorityRecord {
            context_key: actor.identity.authority.work_context.to_string(),
            membership: platform::WorkContextMembershipLevel::Owner,
            policy_revision: "r1".into(),
            owner_kind: platform::ArtifactGrantSubjectKind::Principal,
            owner_key: identity.principal_key,
            initial_grants: vec![],
            classification: None,
            data_labels: vec![],
            invocation_mode: platform::InvocationMode::Direct,
            initiator_key: Some(actor.identity.actor.id.to_string()),
            delegation_id: None,
        },
        context_digest: version.context_digest,
        policy_digest: "b".repeat(64),
        profile_policy_digest: version.profile_policy_digest.unwrap(),
        request_id: uuid::Uuid::now_v7(),
        descriptor: platform::ArtifactUploadDescriptor {
            filename: "large.bin".into(),
            mime_type: "application/octet-stream".into(),
            byte_len: Some(size),
            sha256: None,
        },
        layout: platform::ArtifactUploadLayout {
            part_bytes: 16 * 1024 * 1024,
            max_parts: 10000,
            max_total_bytes: size.max(1),
            parallel_parts: 4,
        },
        state: platform::ArtifactUploadState::Open,
        reserved_bytes: size,
        accepted_bytes: 0,
        accepted_part_count: 0,
        object_key: format!("tenants/acme/uploads/{id}"),
        multipart_id: None,
        generation: 0,
        lease_owner: None,
        lease_until: None,
        manifest: None,
        artifact: platform::ArtifactId::new().record_id(),
        verified_sha256: None,
        completed_at: None,
        failure: None,
        cleanup_pending: false,
        cleanup_bytes: 0,
        inactivity_seconds: 86400,
        created_at: now,
        updated_at: now,
        expires_at: now + TimeDelta::days(1),
        lifetime_ends_at: now + TimeDelta::days(7),
    }
}

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.2.4 process"]
async fn upload_admission_serializes_large_reservations_replays_and_authority_changes() {
    const GIB: i64 = 1024 * 1024 * 1024;
    let mut database = Database::start();
    let store = database.connect().await;
    let replica = database.connect().await;
    let alice = caller("alice", "acme", &[]);
    let context_id = context(&store, &alice).await;
    install_profile(&store).await;
    let draft = admission(&store, &alice, 10 * GIB).await;
    let races = (0..8).map(|index| {
        let db = if index % 2 == 0 { &store } else { &replica };
        let mut request = draft.clone();
        request.id = platform::upload_record_id(uuid::Uuid::now_v7());
        async move {
            db.admit_artifact_upload(request, 12 * GIB, 8)
                .await
                .unwrap()
        }
    });
    let replays = futures::future::join_all(races).await;
    assert!(replays.iter().all(|row| row.id == replays[0].id));
    assert!(replays.iter().all(|row| row.reserved_bytes == 10 * GIB));
    let conflict = {
        let mut value = draft.clone();
        value.descriptor.filename = "different.bin".into();
        value
    };
    assert!(matches!(
        store.admit_artifact_upload(conflict, 12 * GIB, 8).await,
        Err(platform::StoreError::ArtifactUpload(
            platform::ArtifactUploadRejection::Conflict
        ))
    ));
    let next = admission(&replica, &alice, 3 * GIB).await;
    assert!(matches!(
        replica
            .admit_artifact_upload(next.clone(), 12 * GIB, 8)
            .await,
        Err(platform::StoreError::ArtifactUpload(
            platform::ArtifactUploadRejection::Quota
        ))
    ));
    assert!(matches!(
        replica
            .admit_artifact_upload(next.clone(), 100 * GIB, 1)
            .await,
        Err(platform::StoreError::ArtifactUpload(
            platform::ArtifactUploadRejection::Busy
        ))
    ));
    store
        .client()
        .query("UPDATE ONLY $context SET policy_revision = 'r2';")
        .bind(("context", context_id))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        replica.admit_artifact_upload(next, 100 * GIB, 8).await,
        Err(platform::StoreError::ArtifactUpload(
            platform::ArtifactUploadRejection::Denied
        ))
    ));
    database.finish();
}
