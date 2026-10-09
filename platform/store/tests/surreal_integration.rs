use std::collections::BTreeMap;

use chrono::{TimeDelta, Utc};
use uuid::Uuid;
use veoveo_platform_store::{
    ArtifactAccessRequestDecisionDraft, ArtifactAccessRequestDraft, ArtifactAccessRequestId,
    ArtifactAccessRequestQuery, ArtifactAccessRequestState, ArtifactGrantDraft,
    ArtifactGrantSubjectKind, ArtifactId, ArtifactOccurrenceDraft, ArtifactReleaseState,
    ArtifactShareLinkDraft, ArtifactWriteCapabilityDraft, ArtifactWriteCapabilityId,
    ArtifactWriteCapabilityRecord, ArtifactWriteRedemptionId, ChangefeedCursor, ChangefeedEntry,
    GatewayReplayKind, GatewayReplayRecord, GrantPermission, InvocationAuthorityRecord,
    InvocationMode, PlatformIdentity, PlatformStore, PrincipalKind, RecordIdKey, ShareLinkId,
    StoreError, WorkContextInitialGrantRecord, WorkContextMembershipLevel, decode_changefeed_entry,
    deterministic_work_context_id, gateway_replay_record_id,
};

#[path = "surreal_integration/audit_transactions.rs"]
mod audit_transactions;
#[path = "surreal_integration/changefeed.rs"]
mod changefeed;
#[path = "surreal_integration/credentials.rs"]
mod credentials;
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "surreal_integration/query_semantics.rs"]
mod query_semantics;
#[path = "surreal_integration/relationships.rs"]
mod relationships;

fn artifact_audit_context(
    identity: &veoveo_platform_store::PlatformIdentity,
) -> veoveo_platform_store::audit::AuditContextRecord {
    use veoveo_audit_contract::{
        AuditActor, AuditAuthority, AuditContext, AuditPrincipalKind, AuditRequest,
    };
    veoveo_platform_store::audit::AuditContextRecord(AuditContext {
        actor: AuditActor {
            principal: veoveo_types::PrincipalId::parse(identity.principal_key.clone()).unwrap(),
            tenant: Some(veoveo_types::TenantId::parse(identity.tenant_key.clone()).unwrap()),
            kind: AuditPrincipalKind::User,
            oauth_client: None,
            session_family: None,
            delegating_principal: None,
            managed_agent: None,
        },
        authority: AuditAuthority {
            profile: Some("operator".parse().unwrap()),
            work_context: Some(
                veoveo_types::WorkContextId::parse(artifact_authority(identity).context_key)
                    .unwrap(),
            ),
            policy_revision: Some(
                veoveo_types::PolicyVersion::parse(artifact_authority(identity).policy_revision)
                    .unwrap(),
            ),
            ..Default::default()
        },
        request: AuditRequest::background(),
    })
}

fn artifact_authority(identity: &PlatformIdentity) -> InvocationAuthorityRecord {
    InvocationAuthorityRecord {
        context_key: "operations".into(),
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: "r1".into(),
        owner_kind: ArtifactGrantSubjectKind::Principal,
        owner_key: identity.principal_key.clone(),
        initial_grants: vec![WorkContextInitialGrantRecord {
            subject_kind: ArtifactGrantSubjectKind::Principal,
            subject_key: identity.principal_key.clone(),
            permission: GrantPermission::Admin,
        }],
        classification: None,
        data_labels: Vec::new(),
        invocation_mode: InvocationMode::Direct,
        initiator_key: Some(identity.principal_key.clone()),
        delegation_id: None,
    }
}

fn owner_grant(artifact_id: ArtifactId, identity: &PlatformIdentity) -> ArtifactGrantDraft {
    ArtifactGrantDraft {
        artifact_id,
        subject: identity.principal_id.record_id(),
        subject_kind: ArtifactGrantSubjectKind::Principal,
        subject_key: identity.principal_key.clone(),
        permission: GrantPermission::Admin,
        labels: Vec::new(),
        expires_at: None,
        created_by: identity.principal_id,
    }
}

#[tokio::test]
async fn gateway_replay_claim_is_atomic_across_store_instances() {
    let db = fixture::TestDb::new().await;
    let first = db.a.clone();
    let second = db.b.clone();
    let now = Utc::now();
    let record = GatewayReplayRecord {
        id: gateway_replay_record_id(
            GatewayReplayKind::ClientAssertion,
            "authorization-server",
            "client",
            "jwt-id",
        ),
        kind: GatewayReplayKind::ClientAssertion,
        authorization_server: "authorization-server".to_owned(),
        client_id: "client".to_owned(),
        jwt_id: "jwt-id".to_owned(),
        seen_at: now,
        expires_at: now + TimeDelta::minutes(5),
    };

    let (left, right) = tokio::join!(
        first.register_gateway_replay_id(record.clone(), now),
        second.register_gateway_replay_id(record, now),
    );
    let claims = usize::from(left.unwrap()) + usize::from(right.unwrap());
    assert_eq!(claims, 1, "exactly one concurrent replay claim must win");
}

#[tokio::test]
async fn artifact_plane_counters_and_occurrence_dedup_are_durable() {
    let db = fixture::TestDb::new().await;
    let store = db.a.clone();
    let identity = store
        .ensure_identity(
            "tenant-a",
            "alice",
            "https://idp.example.com",
            "alice-subject",
            PrincipalKind::User,
        )
        .await
        .unwrap();
    let first_id = ArtifactId::new();
    let artifact = store
        .create_artifact_occurrence(ArtifactOccurrenceDraft {
            artifact_id: first_id,
            identity: identity.clone(),
            authority: artifact_authority(&identity),
            owner: identity.principal_id.record_id(),
            initial_grants: vec![owner_grant(first_id, &identity)],
            sha256: "a".repeat(64),
            byte_len: 4,
            object_key: "tenants/tenant-a/blobs/opaque-a".into(),
            media_type: "application/octet-stream".into(),
            filename: Some("result.bin".into()),
            classification: String::new(),
            labels: vec![],
            metadata: BTreeMap::new(),
            retention_expires_at: None,
        })
        .await
        .unwrap();
    let second_id = ArtifactId::new();
    let second = store
        .create_artifact_occurrence(ArtifactOccurrenceDraft {
            artifact_id: second_id,
            identity: identity.clone(),
            authority: artifact_authority(&identity),
            owner: identity.principal_id.record_id(),
            initial_grants: vec![owner_grant(second_id, &identity)],
            sha256: "a".repeat(64),
            byte_len: 4,
            object_key: "tenants/tenant-a/blobs/opaque-a".into(),
            media_type: "application/octet-stream".into(),
            filename: None,
            classification: String::new(),
            labels: vec![],
            metadata: BTreeMap::new(),
            retention_expires_at: None,
        })
        .await
        .unwrap();
    assert_ne!(artifact.occurrence.id, second.occurrence.id);
    assert_eq!(artifact.blob.id, second.blob.id);
    assert_eq!(artifact.grants[0].subject_key, "alice");
    let visible = store
        .artifact_read_page(
            veoveo_platform_store::ArtifactReadScope::new(&identity, [], Default::default(), None)
                .unwrap(),
            None,
            10,
        )
        .await
        .unwrap();
    assert_eq!(visible.len(), 2);
    assert!(
        visible
            .iter()
            .any(|item| item.occurrence.id == first_id.record_id())
    );

    let requester = store
        .ensure_identity(
            "tenant-a",
            "bob",
            "https://idp.example.com",
            "bob-subject",
            PrincipalKind::User,
        )
        .await
        .unwrap();
    let request_id = ArtifactAccessRequestId::new();
    let requested = store
        .create_or_reopen_artifact_access_request(ArtifactAccessRequestDraft {
            request_id,
            identity: requester.clone(),
            artifact_id: first_id,
            requested_level: GrantPermission::Read,
            justification: "Assigned to review this output.".into(),
        })
        .await
        .unwrap();
    assert_eq!(requested.state, ArtifactAccessRequestState::Pending);
    let work_context = deterministic_work_context_id("tenant-a", "operations").unwrap();
    let reviewable = store
        .list_artifact_access_requests(ArtifactAccessRequestQuery {
            tenant_id: identity.tenant_id,
            requester_id: None,
            work_context_id: Some(work_context),
            state: Some(ArtifactAccessRequestState::Pending),
            cursor: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert_eq!(reviewable.len(), 1);
    let approved = store
        .decide_artifact_access_request(ArtifactAccessRequestDecisionDraft {
            identity: identity.clone(),
            request_id,
            state: ArtifactAccessRequestState::Approved,
            note: Some("Review assignment verified.".into()),
        })
        .await
        .unwrap();
    assert_eq!(approved.state, ArtifactAccessRequestState::Approved);
    let aggregate = store.artifact_aggregate(first_id).await.unwrap().unwrap();
    assert!(aggregate.grants.iter().any(|grant| {
        grant.subject_key == requester.principal_key && grant.permission == GrantPermission::Read
    }));
    let committed: Vec<veoveo_platform_store::RecordId> = store
        .client()
        .query(include_str!("queries/surreal_integration/artifact_plane_counters_and_occurrence_dedup_are_durable.surql"))
        .await
        .unwrap()
        .check()
        .unwrap()
        .take(0)
        .unwrap();
    assert!(committed.len() >= 2);

    let capability_id = ArtifactWriteCapabilityId::new();
    let capability_task_id = veoveo_artifact_contract::ArtifactTaskId::new();
    store
        .create_artifact_write_capability(ArtifactWriteCapabilityDraft {
            audit: artifact_audit_context(&identity),
            capability_id,
            identity: identity.clone(),
            authority: artifact_authority(&identity),
            profile_key: "operator".into(),
            server_key: "media".into(),
            task_id: capability_task_id,
            actor_kind: PrincipalKind::User,
            actor_issuer: "https://idp.example.com".into(),
            actor_subject: "alice-subject".into(),
            token_hash: "b".repeat(64),
            labels: vec!["cui".into()],
            max_artifact_count: 1,
            max_total_bytes: 4,
            expires_at: Utc::now() + TimeDelta::minutes(5),
        })
        .await
        .unwrap();
    let proposed = first_id;
    assert!(matches!(
        store
            .reserve_artifact_write_capability(
                capability_id,
                &"b".repeat(64),
                &veoveo_artifact_contract::ArtifactTaskId::new(),
                "media:wrong:output:0",
                &"d".repeat(64),
                4,
                &["cui".into()],
                proposed,
            )
            .await,
        Err(StoreError::ArtifactWriteDenied)
    ));
    assert!(matches!(
        store
            .reserve_artifact_write_capability(
                capability_id,
                &"0".repeat(64),
                &capability_task_id,
                "media:wrong-token:output:0",
                &"d".repeat(64),
                4,
                &["cui".into()],
                proposed,
            )
            .await,
        Err(StoreError::ArtifactWriteDenied)
    ));
    assert!(matches!(
        store
            .reserve_artifact_write_capability(
                capability_id,
                &"b".repeat(64),
                &capability_task_id,
                "media:wrong-label:output:0",
                &"d".repeat(64),
                4,
                &["restricted".into()],
                proposed,
            )
            .await,
        Err(StoreError::ArtifactWriteDenied)
    ));
    let reserved = store
        .reserve_artifact_write_capability(
            capability_id,
            &"b".repeat(64),
            &capability_task_id,
            "media:task:output:0",
            &"d".repeat(64),
            4,
            &["cui".into()],
            proposed,
        )
        .await
        .unwrap();
    for (token, task, labels) in [
        ("0".repeat(64), capability_task_id, vec!["cui".into()]),
        (
            "b".repeat(64),
            veoveo_artifact_contract::ArtifactTaskId::new(),
            vec!["cui".into()],
        ),
        (
            "b".repeat(64),
            capability_task_id,
            vec!["restricted".into()],
        ),
    ] {
        assert!(matches!(
            store
                .reserve_artifact_write_capability(
                    capability_id,
                    &token,
                    &task,
                    "media:task:output:0",
                    &"d".repeat(64),
                    4,
                    &labels,
                    ArtifactId::new(),
                )
                .await,
            Err(StoreError::ArtifactWriteDenied)
        ));
    }
    let retry = store
        .reserve_artifact_write_capability(
            capability_id,
            &"b".repeat(64),
            &capability_task_id,
            "media:task:output:0",
            &"d".repeat(64),
            4,
            &["cui".into()],
            ArtifactId::new(),
        )
        .await
        .unwrap();
    assert_eq!(retry.redemption.id, reserved.redemption.id);
    assert_eq!(retry.redemption.artifact, reserved.redemption.artifact);
    let mismatched_after_stage = store
        .reserve_artifact_write_capability(
            capability_id,
            &"b".repeat(64),
            &capability_task_id,
            "media:task:output:0",
            &"e".repeat(64),
            4,
            &["cui".into()],
            ArtifactId::new(),
        )
        .await
        .unwrap();
    assert!(!mismatched_after_stage.request_matches);
    assert_eq!(
        mismatched_after_stage.redemption.artifact,
        first_id.record_id()
    );
    let redemption_id = ArtifactWriteRedemptionId::from_uuid(match &reserved.redemption.id.key {
        RecordIdKey::Uuid(value) => **value,
        other => panic!("unexpected redemption id key: {other:?}"),
    });
    assert!(
        store
            .finalize_artifact_write_capability(redemption_id, first_id,)
            .await
            .unwrap()
    );
    assert!(
        !store
            .finalize_artifact_write_capability(redemption_id, first_id)
            .await
            .unwrap()
    );
    assert!(matches!(
        store
            .reserve_artifact_write_capability(
                capability_id,
                &"b".repeat(64),
                &capability_task_id,
                "media:task:output:1",
                &"f".repeat(64),
                1,
                &["cui".into()],
                ArtifactId::new(),
            )
            .await,
        Err(StoreError::ArtifactWriteDenied)
    ));

    let rebind_capability_id = ArtifactWriteCapabilityId::new();
    let rebind_task_id = veoveo_artifact_contract::ArtifactTaskId::new();
    store
        .create_artifact_write_capability(ArtifactWriteCapabilityDraft {
            audit: artifact_audit_context(&identity),
            capability_id: rebind_capability_id,
            identity: identity.clone(),
            authority: artifact_authority(&identity),
            profile_key: "operator".into(),
            server_key: "optimization".into(),
            task_id: rebind_task_id,
            actor_kind: PrincipalKind::User,
            actor_issuer: "https://idp.example.com".into(),
            actor_subject: "alice-subject".into(),
            token_hash: "9".repeat(64),
            labels: vec!["cui".into()],
            max_artifact_count: 2,
            max_total_bytes: 6,
            expires_at: Utc::now() + TimeDelta::minutes(5),
        })
        .await
        .unwrap();
    let rebind_artifact_id = ArtifactId::new();
    let first_reservation = store
        .reserve_artifact_write_capability(
            rebind_capability_id,
            &"9".repeat(64),
            &rebind_task_id,
            "optimization:task:artifact:0",
            &"1".repeat(64),
            4,
            &["cui".into()],
            rebind_artifact_id,
        )
        .await
        .unwrap();
    let rebound = store
        .reserve_artifact_write_capability(
            rebind_capability_id,
            &"9".repeat(64),
            &rebind_task_id,
            "optimization:task:artifact:0",
            &"2".repeat(64),
            6,
            &["cui".into()],
            ArtifactId::new(),
        )
        .await
        .unwrap();
    assert!(rebound.request_matches);
    assert_eq!(rebound.redemption.id, first_reservation.redemption.id);
    assert_eq!(rebound.redemption.artifact, rebind_artifact_id.record_id());
    assert_eq!(rebound.redemption.request_hash, "2".repeat(64));
    assert_eq!(rebound.redemption.byte_len, 6);
    let mut response = store
        .client()
        .query(include_str!("queries/surreal_integration/artifact_plane_counters_and_occurrence_dedup_are_durable_2.surql"))
        .bind(("capability", rebind_capability_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let rebind_capability: ArtifactWriteCapabilityRecord =
        response.take::<Option<_>>(0).unwrap().unwrap();
    assert_eq!(rebind_capability.used_artifact_count, 1);
    assert_eq!(rebind_capability.used_total_bytes, 6);
    assert!(matches!(
        store
            .reserve_artifact_write_capability(
                rebind_capability_id,
                &"9".repeat(64),
                &rebind_task_id,
                "optimization:task:artifact:1",
                &"3".repeat(64),
                1,
                &["cui".into()],
                ArtifactId::new(),
            )
            .await,
        Err(StoreError::ArtifactWriteDenied)
    ));

    let link_id = ShareLinkId::new();
    store
        .create_artifact_share_link(ArtifactShareLinkDraft {
            link_id,
            artifact_id: first_id,
            identity,
            token_hash: "c".repeat(64),
            expires_at: Utc::now() + TimeDelta::minutes(5),
            max_downloads: Some(1),
        })
        .await
        .unwrap();
    assert!(
        store
            .redeem_public_share_link(&"c".repeat(64))
            .await
            .unwrap()
            .is_none(),
        "private artifacts must not redeem public links"
    );
    let released = store
        .set_artifact_release_state(first_id, ArtifactReleaseState::Releasable)
        .await
        .unwrap()
        .expect("release-state mutation returns its committed occurrence");
    assert_eq!(released.id, first_id.record_id());
    assert_eq!(released.release_state, ArtifactReleaseState::Releasable);
    assert!(
        store
            .redeem_public_share_link(&"c".repeat(64))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .redeem_public_share_link(&"c".repeat(64))
            .await
            .unwrap()
            .is_none(),
        "share max_downloads must be atomic"
    );
    assert!(
        !store
            .revoke_artifact_share_link(link_id, ArtifactId::new())
            .await
            .unwrap()
    );
    assert!(
        store
            .revoke_artifact_share_link(link_id, first_id)
            .await
            .unwrap()
    );
    assert!(
        !store
            .revoke_artifact_share_link(link_id, first_id)
            .await
            .unwrap()
    );
}

/// Pins the SurrealDB changefeed contract the console stream depends on:
/// the oracle versionstamp layout (`unix_millis << 16`), `INCLUDE ORIGINAL`
/// entry shapes, the delete shape (record id + original row), gap-free
/// resume from a versionstamp, and cross-table versionstamp comparability.
/// Datetime `SINCE` is intentionally NOT used: it returns nothing on this
/// deployment, which is why cursors are clock-anchored versionstamps.
/// Run explicitly with:
/// `VEOVEO_SURREAL_INTEGRATION=1 cargo test -p veoveo-platform-store --test surreal_integration`
#[tokio::test]
async fn changefeed_replay_contract_is_pinned() {
    let db = fixture::TestDb::new().await;
    let store = db.a.clone();

    let db_now = |store: &PlatformStore| {
        let store = store.clone();
        async move {
            let mut response = store
                .client()
                .query(include_str!(
                    "queries/surreal_integration/changefeed_replay_contract_is_pinned.surql"
                ))
                .await
                .unwrap()
                .check()
                .unwrap();
            let now: surrealdb::types::Value = response.take(0).unwrap();
            let surrealdb::types::Value::Datetime(now) = now else {
                panic!("time::now() must return a datetime");
            };
            now.into_inner().timestamp_millis()
        }
    };

    // Anchor a cursor on the database clock BEFORE any writes, exactly as
    // the console snapshot handler will before reading its projection.
    let anchor = store.changefeed_cursor_now().await.unwrap();
    let db_before_writes_ms = db_now(&store).await;

    let first = store
        .ensure_identity(
            "tenant-changefeed",
            "cf-first",
            "https://veoveo.local/tests",
            "cf-first",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    let second = store
        .ensure_identity(
            "tenant-changefeed",
            "cf-second",
            "https://veoveo.local/tests",
            "cf-second",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    store
        .client()
        .query(include_str!(
            "queries/surreal_integration/changefeed_replay_contract_is_pinned_2.surql"
        ))
        .bind(("principal", first.principal_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    store
        .client()
        .query(include_str!(
            "queries/surreal_integration/changefeed_replay_contract_is_pinned_3.surql"
        ))
        .bind(("principal", second.principal_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let db_after_writes_ms = db_now(&store).await;

    let batches = store.replay_changes(anchor, 1000).await.unwrap();
    assert!(
        !batches.is_empty(),
        "a clock-anchored cursor must surface the principal mutations"
    );
    let versionstamps: Vec<i64> = batches.iter().map(|batch| batch.versionstamp).collect();
    assert!(
        versionstamps.windows(2).all(|pair| pair[0] <= pair[1]),
        "versionstamps must be monotonic: {versionstamps:?}"
    );

    // Pin the oracle layout the clock anchoring depends on. A SurrealDB
    // upgrade that changes the layout must fail here, not in production.
    let last_versionstamp = *versionstamps.last().unwrap();
    let last_millis = last_versionstamp >> 16;
    assert_eq!(
        veoveo_platform_store::ChangefeedCursor::from_versionstamp(last_versionstamp)
            .unwrap()
            .timestamp()
            .unwrap()
            .timestamp_millis(),
        last_millis
    );
    assert!(
        (db_before_writes_ms - 1_000..=db_after_writes_ms + 1_000).contains(&last_millis),
        "versionstamp >> 16 must be unix millis: {last_millis} outside          [{db_before_writes_ms}, {db_after_writes_ms}]"
    );

    // Every entry must decode: unknown shapes would silently drop console rows.
    let mut saw_first_create = false;
    let mut update_versionstamp = None;
    let mut delete_original_tenant = None;
    for batch in &batches {
        for change in &batch.changes {
            match decode_changefeed_entry(change).expect("all changefeed entries must decode") {
                ChangefeedEntry::Upsert(row) => {
                    let surrealdb::types::Value::RecordId(record) = row.get("id").clone() else {
                        panic!("upsert row without record id: {row:?}");
                    };
                    if record == first.principal_id.record_id() {
                        saw_first_create = true;
                        if row.get("display_name")
                            == surrealdb::types::Value::String("Changefeed Probe".to_owned())
                        {
                            update_versionstamp = Some(batch.versionstamp);
                        }
                    }
                }
                ChangefeedEntry::Delete { record, original } => {
                    if record == second.principal_id.record_id() {
                        let original =
                            original.expect("INCLUDE ORIGINAL deletes must carry the original row");
                        delete_original_tenant = Some(original.get("tenant").clone());
                    }
                }
                ChangefeedEntry::Definition => {}
            }
        }
    }
    assert!(
        saw_first_create,
        "create of the first principal must replay as an upsert"
    );
    let update_versionstamp = update_versionstamp.expect(
        "the display_name update must replay as a full-row upsert (INCLUDE ORIGINAL current form)",
    );
    let delete_original_tenant =
        delete_original_tenant.expect("the delete of the second principal must replay");
    assert_eq!(
        delete_original_tenant,
        surrealdb::types::Value::RecordId(second.tenant_id.record_id()),
        "delete originals must expose the tenant for content-based filtering"
    );

    // Resuming from the versionstamp before the tail must redeliver the tail
    // (gap-free resume; redelivery of the cursor batch itself is acceptable).
    if let Some(&resume_from) = versionstamps
        .iter()
        .rev()
        .find(|stamp| **stamp < last_versionstamp)
    {
        let resumed = store
            .replay_changes(
                ChangefeedCursor::from_versionstamp(resume_from).unwrap(),
                1000,
            )
            .await
            .unwrap();
        assert!(
            resumed
                .iter()
                .any(|batch| batch.versionstamp == last_versionstamp),
            "resume from {resume_from} must redeliver the tail batch {last_versionstamp}"
        );
    }

    // Versionstamps must be comparable across tables so multi-table replay
    // batches can be merge-sorted into one ordered stream.
    let tenant_batches = store.replay_changes(anchor, 1000).await.unwrap();
    let tenant_max = tenant_batches
        .iter()
        .filter(|batch| {
            batch
                .changes
                .iter()
                .any(|change| decode_changefeed_entry(change).unwrap().table() == Some("tenant"))
        })
        .map(|batch| batch.versionstamp)
        .max()
        .expect("tenant creation must appear in its changefeed");
    assert!(
        tenant_max <= update_versionstamp,
        "tenant creation ({tenant_max}) must order before the later principal update \
         ({update_versionstamp}) across tables"
    );
}

#[path = "surreal_integration/gateway_snapshots.rs"]
mod gateway_snapshots;
