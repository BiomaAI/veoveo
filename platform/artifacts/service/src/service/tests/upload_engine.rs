//! Full service transfer and recovery against native Store and a shared byte backend.

use super::native_database::{Database, context};
use super::upload_admission::install_profile_policy;
use super::*;
use crate::{ArtifactObjectStore, uploads::UploadService};
use axum::body::Bytes;
use std::{num::NonZeroU32, sync::Arc, time::Duration};
use veoveo_mcp_contract as contract;
use veoveo_platform_store as platform;

async fn fixture(
    store: &platform::PlatformStore,
    actor: &PlaneCaller,
) -> contract::VerifiedArtifactUploadIdentity {
    context(store, actor).await;
    let policy = veoveo_artifact_contract::ArtifactUploadPolicy {
        max_object_bytes: std::num::NonZeroU64::new(107374182400).unwrap(),
        tenant_quota_bytes: std::num::NonZeroU64::new(214748364800).unwrap(),
        max_active_uploads_per_tenant: NonZeroU32::new(8).unwrap(),
        part_bytes: std::num::NonZeroU64::new(16777216).unwrap(),
        max_part_bytes: std::num::NonZeroU64::new(67108864).unwrap(),
        max_parts: NonZeroU32::new(10000).unwrap(),
        parallel_parts: NonZeroU32::new(4).unwrap(),
        max_inflight_bytes: std::num::NonZeroU64::new(134217728).unwrap(),
        inactivity_seconds: std::num::NonZeroU64::new(86400).unwrap(),
        lifetime_seconds: std::num::NonZeroU64::new(604800).unwrap(),
        part_timeout_seconds: std::num::NonZeroU64::new(120).unwrap(),
        allowed_mime_types: std::collections::BTreeSet::from([
            "application/octet-stream".to_owned()
        ]),
    };
    install_profile_policy(store, Some(policy)).await;
    let version = store
        .artifact_upload_authority_version(
            "acme",
            actor.identity.authority.work_context.as_str(),
            "fixture",
        )
        .await
        .unwrap()
        .unwrap();
    let mut identity = actor.identity.clone();
    identity.server =
        contract::ServerSlug::parse(veoveo_artifact_contract::ARTIFACT_UPLOAD_AUDIENCE).unwrap();
    identity.profile = contract::GatewayProfileId::parse("fixture").unwrap();
    identity
        .actor
        .scopes
        .insert(veoveo_types::ScopeName::parse("artifact:upload").unwrap());
    bind_request_context(&mut identity);
    contract::VerifiedArtifactUploadIdentity {
        identity,
        authorization: veoveo_artifact_contract::ArtifactUploadAuthority {
            control_plane_sha256: veoveo_artifact_contract::UploadSha256::parse(
                version.control_plane_sha256,
            )
            .unwrap(),
            context_digest: veoveo_artifact_contract::UploadSha256::parse(version.context_digest)
                .unwrap(),
        },
    }
}

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns native database and HTTP listener"]
async fn upload_http_enforces_identity_and_streams_to_a_durable_receipt() {
    use contract::{GatewayInternalTokenIssuer, GatewayInternalTokenVerifier};
    use veoveo_artifact_contract::{ArtifactUploadSession, ArtifactUploadState};
    let mut database = Database::start();
    let store = database.connect().await;
    let actor = caller("alice", "acme", &[]);
    let verified = fixture(&store, &actor).await;
    let issuer_id = contract::TokenIssuer::parse("veoveo-internal").unwrap();
    let issuer =
        GatewayInternalTokenIssuer::new(issuer_id.clone(), crate::http::tests::signing_key());
    let token = issuer
        .issue_artifact_upload(
            verified.identity.profile.clone(),
            verified.identity.actor.clone(),
            verified.identity.authority.clone(),
            verified.identity.request_context.clone().unwrap(),
            verified.authorization.clone(),
            Utc::now() + TimeDelta::minutes(5),
        )
        .unwrap();
    let verifier = GatewayInternalTokenVerifier::new(
        issuer_id,
        contract::ServerSlug::parse(veoveo_artifact_contract::ARTIFACT_UPLOAD_AUDIENCE).unwrap(),
        crate::http::tests::trust_bundle(),
    );
    let service = UploadService::new(
        store,
        ArtifactObjectStore::with_multipart(Arc::new(object_store::memory::InMemory::new())),
    );
    let app = crate::http::uploads::router(service.clone(), verifier);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/artifact-uploads", listener.local_addr().unwrap());
    let http = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let recovery = tokio::spawn(service.run_recovery());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    assert_eq!(
        client
            .post(&url)
            .body("unread")
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let ordinary = issuer
        .issue(
            verified.identity.profile.clone(),
            contract::ServerSlug::parse("artifact").unwrap(),
            verified.identity.actor.clone(),
            verified.identity.authority.clone(),
            None,
            Utc::now() + TimeDelta::minutes(5),
        )
        .unwrap();
    assert_eq!(
        client
            .get(format!("{url}/policy"))
            .bearer_auth(ordinary.bearer_token)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let policy = client
        .get(format!("{url}/policy"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(policy.status(), 200);
    assert_eq!(policy.headers()["cache-control"], "no-store");
    let key = veoveo_artifact_contract::ArtifactUploadRequestId::new().to_string();
    let data = vec![42_u8; 131_071];
    let sha = hex::encode(Sha256::digest(&data));
    let mut invalid = serde_json::to_value(descriptor(data.len())).unwrap();
    invalid["owner"] = serde_json::json!("someone-else");
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(&token)
            .header("idempotency-key", &key)
            .json(&invalid)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let admitted = client
        .post(&url)
        .bearer_auth(&token)
        .header("idempotency-key", &key)
        .json(&descriptor(data.len()))
        .send()
        .await
        .unwrap();
    assert_eq!(admitted.status(), 201);
    let admitted: ArtifactUploadSession = admitted.json().await.unwrap();
    let replay = client
        .post(&url)
        .bearer_auth(&token)
        .header("idempotency-key", &key)
        .json(&descriptor(data.len()))
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), 200);
    assert_eq!(
        replay
            .json::<ArtifactUploadSession>()
            .await
            .unwrap()
            .upload_id,
        admitted.upload_id
    );
    let session_url = format!("{url}/{}", admitted.upload_id);
    let part_url = format!("{session_url}/parts/1");
    assert_eq!(
        client
            .put(&part_url)
            .bearer_auth(&token)
            .header(
                veoveo_artifact_contract::UPLOAD_PART_BYTE_LEN_HEADER,
                data.len() + 1
            )
            .header(veoveo_artifact_contract::UPLOAD_PART_SHA256_HEADER, &sha)
            .body(data.clone())
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let part = client
        .put(&part_url)
        .bearer_auth(&token)
        .header(
            veoveo_artifact_contract::UPLOAD_PART_BYTE_LEN_HEADER,
            data.len(),
        )
        .header(veoveo_artifact_contract::UPLOAD_PART_SHA256_HEADER, &sha)
        .body(data.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(part.status(), 200);
    assert_eq!(
        part.json::<veoveo_artifact_contract::UploadPartReceipt>()
            .await
            .unwrap()
            .byte_len,
        data.len() as u64
    );
    let mut foreign = verified.identity.clone();
    foreign.actor.id = veoveo_types::PrincipalId::parse("another-person").unwrap();
    foreign.authority.provenance = InvocationProvenance::Direct {
        initiator: foreign.actor.id.clone(),
    };
    bind_request_context(&mut foreign);
    let foreign = issuer
        .issue_artifact_upload(
            verified.identity.profile.clone(),
            foreign.actor.clone(),
            foreign.authority.clone(),
            foreign.request_context.clone().unwrap(),
            verified.authorization.clone(),
            Utc::now() + TimeDelta::minutes(5),
        )
        .unwrap();
    assert_eq!(
        client
            .get(&session_url)
            .bearer_auth(foreign)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let manifest = veoveo_artifact_contract::CompleteArtifactUpload {
        byte_len: data.len() as u64,
        part_count: NonZeroU32::new(1).unwrap(),
        sha256: Some(veoveo_artifact_contract::UploadSha256::parse(&sha).unwrap()),
    };
    let completion_url = format!("{session_url}/complete");
    let completion = client
        .post(&completion_url)
        .bearer_auth(&token)
        .json(&manifest)
        .send()
        .await
        .unwrap();
    assert_eq!(completion.status(), 202);
    assert!(
        completion
            .json::<ArtifactUploadSession>()
            .await
            .unwrap()
            .receipt
            .is_none()
    );
    let completed = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let status = client
                .get(&session_url)
                .bearer_auth(&token)
                .send()
                .await
                .unwrap();
            assert_eq!(status.status(), 200);
            let status: ArtifactUploadSession = status.json().await.unwrap();
            if status.state == ArtifactUploadState::Completed {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let receipt = completed.receipt.unwrap();
    assert_eq!(receipt.byte_len, data.len() as u64);
    assert_eq!(receipt.sha256.as_str(), sha);
    assert_eq!(
        client
            .post(completion_url)
            .bearer_auth(&token)
            .json(&manifest)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .delete(&session_url)
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    http.abort();
    recovery.abort();
    let _ = http.await;
    let _ = recovery.await;
    database.finish();
}

fn stream(bytes: Vec<u8>) -> BlobStream {
    let chunks: Vec<_> = bytes
        .chunks(8192)
        .map(|bytes| Ok(Bytes::copy_from_slice(bytes)))
        .collect();
    Box::pin(futures::stream::iter(chunks))
}

fn descriptor(bytes: usize) -> veoveo_artifact_contract::CreateArtifactUpload {
    veoveo_artifact_contract::CreateArtifactUpload {
        filename: "measurements.bin".into(),
        mime_type: "application/octet-stream".into(),
        byte_len: Some(bytes as u64),
        sha256: None,
    }
}

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.3.0 process"]
async fn upload_service_replays_admission_and_parts_then_recovers_completion_on_another_replica() {
    let mut database = Database::start();
    let store = database.connect().await;
    let actor = caller("alice", "acme", &[]);
    let verified = fixture(&store, &actor).await;
    let objects =
        ArtifactObjectStore::with_multipart(Arc::new(object_store::memory::InMemory::new()));
    let service = UploadService::new(store.clone(), objects.clone());
    let policy = service.policy(&verified).await.unwrap();
    assert!(policy.allowed);
    assert_eq!(policy.available_bytes, Some(214748364800));
    let bytes: Vec<_> = (0..262144).map(|i| (i % 251) as u8).collect();
    let sha =
        veoveo_artifact_contract::UploadSha256::parse(hex::encode(Sha256::digest(&bytes))).unwrap();
    let key = veoveo_artifact_contract::ArtifactUploadRequestId::new();
    let (session, created) = service
        .create(&verified, key, descriptor(bytes.len()))
        .await
        .unwrap();
    assert!(created);
    let (replay, created) = service
        .create(&verified, key, descriptor(bytes.len()))
        .await
        .unwrap();
    assert!(!created);
    assert_eq!(session.upload_id, replay.upload_id);
    let mut foreign = verified.clone();
    foreign.identity.actor.id = veoveo_types::PrincipalId::parse("another-person").unwrap();
    assert!(matches!(
        service.status(&foreign, session.upload_id, 0).await,
        Err(crate::uploads::UploadFault(
            veoveo_artifact_contract::UploadErrorCode::NotFound
        ))
    ));
    let part = service
        .put_part(
            &verified,
            session.upload_id,
            NonZeroU32::new(1).unwrap(),
            bytes.len() as u64,
            sha.clone(),
            stream(bytes.clone()),
        )
        .await
        .unwrap();
    let repeated = service
        .put_part(
            &verified,
            session.upload_id,
            NonZeroU32::new(1).unwrap(),
            bytes.len() as u64,
            sha.clone(),
            stream(vec![]),
        )
        .await
        .unwrap();
    assert_eq!(part, repeated);
    let complete = veoveo_artifact_contract::CompleteArtifactUpload {
        byte_len: bytes.len() as u64,
        part_count: NonZeroU32::new(1).unwrap(),
        sha256: Some(sha.clone()),
    };
    let finalizing = service
        .complete(&verified, session.upload_id, complete.clone())
        .await
        .unwrap();
    assert_eq!(
        finalizing.state,
        veoveo_artifact_contract::ArtifactUploadState::Finalizing
    );
    assert_eq!(finalizing.accepted_bytes, bytes.len() as u64);
    assert!(finalizing.receipt.is_none());
    drop(service);
    let replica = UploadService::new(database.connect().await, objects.clone());
    let recovery = tokio::spawn(replica.clone().run_recovery());
    let completed = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let row = replica
                .status(&verified, session.upload_id, 0)
                .await
                .unwrap();
            if row.state == veoveo_artifact_contract::ArtifactUploadState::Completed {
                break row;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("durable completion did not recover");
    let receipt = completed.receipt.unwrap();
    assert_eq!(receipt.sha256, sha);
    assert_eq!(receipt.byte_len, bytes.len() as u64);
    let again = replica
        .complete(&verified, session.upload_id, complete)
        .await
        .unwrap()
        .receipt
        .unwrap();
    assert_eq!(receipt, again);
    let ordinary = ArtifactService::new(
        crate::SurrealArtifactRepository::new(store.clone()),
        objects,
    );
    let downloaded = ordinary
        .get(&actor, &receipt.artifact_id, AccessLevel::Read)
        .await
        .unwrap();
    assert_eq!(downloaded.bytes, bytes);
    let serialized = serde_json::to_string(&replay).unwrap();
    assert!(!serialized.contains("multipart"));
    assert!(!serialized.contains("object_key"));
    recovery.abort();
    let _ = recovery.await;
    database.finish();
}

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.3.0 process"]
async fn dropped_upload_request_releases_memory_and_cancellation_recovers_physical_cleanup() {
    let mut database = Database::start();
    let store = database.connect().await;
    let actor = caller("alice", "acme", &[]);
    let verified = fixture(&store, &actor).await;
    let objects =
        ArtifactObjectStore::with_multipart(Arc::new(object_store::memory::InMemory::new()));
    let service = UploadService::new(store.clone(), objects.clone());
    let (session, _) = service
        .create(
            &verified,
            veoveo_artifact_contract::ArtifactUploadRequestId::new(),
            descriptor(1024),
        )
        .await
        .unwrap();
    let copy = service.clone();
    let identity = verified.clone();
    let request = tokio::spawn(async move {
        copy.put_part(
            &identity,
            session.upload_id,
            NonZeroU32::new(1).unwrap(),
            1024,
            veoveo_artifact_contract::UploadSha256::parse("a".repeat(64)).unwrap(),
            Box::pin(futures::stream::pending()),
        )
        .await
    });
    let tenant = platform::deterministic_tenant_id("acme").unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while store
            .artifact_upload_storage_usage(tenant)
            .await
            .unwrap()
            .unwrap()
            .inflight_bytes
            == 0
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    request.abort();
    let _ = request.await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while store
            .artifact_upload_storage_usage(tenant)
            .await
            .unwrap()
            .unwrap()
            .inflight_bytes
            != 0
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("dropped request retained its transfer budget");
    service.cancel(&verified, session.upload_id).await.unwrap();
    let recovery = tokio::spawn(service.clone().run_recovery());
    tokio::time::timeout(Duration::from_secs(20), async {
        while store
            .artifact_upload(session.upload_id.as_uuid())
            .await
            .unwrap()
            .unwrap()
            .cleanup_pending
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("cancelled upload cleanup was not recovered");
    let usage = store
        .artifact_upload_storage_usage(tenant)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            usage.reserved_bytes,
            usage.cleanup_bytes,
            usage.inflight_bytes,
            usage.active_uploads
        ),
        (0, 0, 0, 0)
    );
    let mut stale = verified.clone();
    stale.authorization.control_plane_sha256 =
        veoveo_artifact_contract::UploadSha256::parse("d".repeat(64)).unwrap();
    assert!(matches!(
        service
            .create(
                &stale,
                veoveo_artifact_contract::ArtifactUploadRequestId::new(),
                descriptor(1)
            )
            .await,
        Err(crate::uploads::UploadFault(
            veoveo_artifact_contract::UploadErrorCode::Denied
        ))
    ));
    recovery.abort();
    let _ = recovery.await;
    database.finish();
}

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.3.0 process"]
async fn upload_ownership_filters_foreign_malformed_rows_before_decoding() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let mut database = Database::start();
        let store = database.connect().await;
        let actor = caller("alice", "acme", &[]);
        let verified = fixture(&store, &actor).await;
        let service = UploadService::new(store.clone(),
            ArtifactObjectStore::with_multipart(Arc::new(object_store::memory::InMemory::new())));
        let (session, _) = service.create(&verified, veoveo_artifact_contract::ArtifactUploadRequestId::new(), descriptor(1)).await.unwrap();
        service.status(&verified, session.upload_id, 0).await.unwrap();
        store.client()
            .query(include_str!("../../../tests/queries/service/tests/upload_engine/upload_ownership_filters_foreign_malformed_rows_before_decoding.surql"))
            .bind(("upload", platform::upload_record_id(session.upload_id.as_uuid())))
            .await.unwrap().check().unwrap();
        for field in ["tenant", "actor", "profile", "context", "issuer", "subject", "missing_tenant"] {
            let mut foreign = verified.clone();
            match field {
                "tenant" => foreign.identity.actor.tenant = Some(veoveo_types::TenantId::parse("other-tenant").unwrap()),
                "actor" => foreign.identity.actor.id = veoveo_types::PrincipalId::parse("other-actor").unwrap(),
                "profile" => foreign.identity.profile = veoveo_types::GatewayProfileId::parse("other-profile").unwrap(),
                "context" => foreign.identity.authority.work_context = veoveo_types::WorkContextId::parse("other-context").unwrap(),
                "issuer" => foreign.identity.actor.issuer = veoveo_types::TokenIssuer::parse("https://other.example").unwrap(),
                "subject" => foreign.identity.actor.subject = veoveo_types::TokenSubject::parse("other-subject").unwrap(),
                "missing_tenant" => foreign.identity.actor.tenant = None,
                _ => unreachable!(),
            }
            let result = service.status(&foreign, session.upload_id, 0).await;
            assert!(matches!(result, Err(crate::uploads::UploadFault(veoveo_artifact_contract::UploadErrorCode::NotFound))), "foreign {field}: {result:?}");
        }
        // A malformed owned row must still fail; the test must not mask corruption.
        assert!(matches!(service.status(&verified, session.upload_id, 0).await,
            Err(crate::uploads::UploadFault(veoveo_artifact_contract::UploadErrorCode::Unavailable))));
        database.finish();
    }).await.expect("upload ownership qualification exceeded 90 seconds");
}

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.3.0 process"]
async fn upload_checks_profile_lookup_before_initial_admission_and_retained_access() {
    use platform::Value;
    tokio::time::timeout(Duration::from_secs(90), async {
        let mut database = Database::start();
        let store = database.connect().await;
        let actor = caller("alice", "acme", &[]);
        let verified = fixture(&store, &actor).await;
        let service = UploadService::new(store.clone(), ArtifactObjectStore::with_multipart(Arc::new(object_store::memory::InMemory::new())));
        let (session, _) = service.create(&verified, veoveo_artifact_contract::ArtifactUploadRequestId::new(), descriptor(1)).await.unwrap();
        let mut response = store.client().query(include_str!("../../../tests/queries/service/tests/upload_engine/profile_lookup_read.surql")).await.unwrap().check().unwrap();
        let rows: Vec<Value> = response.take(0).unwrap();
        assert_eq!(rows.len(), 1);
        let Value::Object(original) = rows.into_iter().next().unwrap() else { panic!("profile object") };
        let id = original.get("id").unwrap().clone();
        for lookup in [Value::None, Value::Null, Value::String("invalid policy".to_owned())] {
            let mut bad = original.clone();
            bad.insert("profile_policy_version", lookup);
            assert!(store.client().query(include_str!("../../../tests/queries/service/tests/upload_engine/profile_lookup_replace.surql")).bind(("id", id.clone())).bind(("row", Value::Object(bad))).await.unwrap().check().is_err(), "missing/malformed profile lookup must reject atomically");
            assert!(service.policy(&verified).await.unwrap().allowed);
        }

        let revision = match original.get("revision").unwrap() { Value::RecordId(revision) => revision.clone(), _ => panic!("profile revision reference") };
        let other_policy = platform::GatewayControlObjectContent {
            revision, tenant: None, object_kind: "policy".into(), object_id: "different-valid-policy".into(), profile_policy_version: None,
            document: platform::OpenObject::new(std::collections::BTreeMap::from([("version".into(), serde_json::json!("different-valid-policy"))])),
        };
        store.client().query(include_str!("../../../tests/queries/service/tests/upload_engine/profile_lookup_other_policy.surql"))
            .bind(("policy", other_policy)).await.unwrap().check().unwrap();
        let mut inconsistent = original.clone();
        inconsistent.insert("profile_policy_version", Value::String("different-valid-policy".to_owned()));
        store.client().query(include_str!("../../../tests/queries/service/tests/upload_engine/profile_lookup_replace.surql")).bind(("id", id.clone())).bind(("row", Value::Object(inconsistent))).await.unwrap().check().unwrap();
        assert!(store.artifact_upload_authority_version("acme", verified.identity.authority.work_context.as_str(), "fixture").await.unwrap().unwrap().profile_policy_digest.is_some(), "a different valid policy still produces a digest; initial admission must check document agreement");
        for result in [service.policy(&verified).await.map(|_| ()), service.create(&verified, veoveo_artifact_contract::ArtifactUploadRequestId::new(), descriptor(1)).await.map(|_| ()), service.status(&verified, session.upload_id, 0).await.map(|_| ())] {
            assert!(matches!(result, Err(crate::uploads::UploadFault(veoveo_artifact_contract::UploadErrorCode::Unavailable))), "inconsistent metadata must fail before new admission and retained access");
        }
        store.client().query(include_str!("../../../tests/queries/service/tests/upload_engine/profile_lookup_replace.surql")).bind(("id", id)).bind(("row", Value::Object(original))).await.unwrap().check().unwrap();
        assert!(service.policy(&verified).await.unwrap().allowed);
        service.status(&verified, session.upload_id, 0).await.unwrap();
        database.finish();
    }).await.expect("profile lookup admission exceeded 90 seconds");
}
