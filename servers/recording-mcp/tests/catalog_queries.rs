//! Native SQL and service qualification; one disposable Store and no GPU or cluster.
use chrono::{TimeDelta, Utc};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    GatewayInternalIdentity, GatewayProfileId, JwtId, Principal, PrincipalKind, ServerSlug,
    TokenIssuer, TokenSubject,
};
use veoveo_platform_store::{
    RecordingDatasetDraft, RecordingDatasetId, RecordingDraft, RecordingId, RecordingLayerCounts,
    RecordingLayerDraft, RecordingReadScope,
};
use veoveo_recording_mcp::{
    RecordingService,
    contract::{RecordingResource, RecordingScope},
};
use veoveo_recording_reader::access::record_uuid;
use veoveo_types::ResourceAddress;
use veoveo_types::{
    AccessSubject, DataLabelId, InvocationProvenance, PolicyVersion, PrincipalId, TenantId,
    WorkContextId,
};
use veoveo_types::{InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy};

#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "catalog_queries/grants.rs"]
mod grants;
#[path = "catalog_queries/projections.rs"]
mod projections;

fn identity(tenant: &str, name: &str, labels: &[&str]) -> GatewayInternalIdentity {
    let principal = PrincipalId::new(name).unwrap();
    let tenant = TenantId::new(tenant).unwrap();
    let now = Utc::now();
    GatewayInternalIdentity {
        issuer: TokenIssuer::new("https://gateway.example").unwrap(),
        profile: GatewayProfileId::new("recording-test").unwrap(),
        server: ServerSlug::new("recording").unwrap(),
        actor: Principal {
            id: principal.clone(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://identity.example").unwrap(),
            subject: TokenSubject::new(name).unwrap(),
            tenant: Some(tenant.clone()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: None,
            data_labels: labels
                .iter()
                .map(|label| DataLabelId::new(*label).unwrap())
                .collect(),
        },
        authority: InvocationAuthority {
            work_context: WorkContextId::new("operations").unwrap(),
            tenant,
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::new("r1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(principal.clone()),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Direct {
                initiator: principal,
            },
        },
        request_context: None,
        jwt_id: JwtId::new(uuid::Uuid::now_v7().to_string()).unwrap(),
        issued_at: now,
        not_before: now,
        expires_at: now + TimeDelta::minutes(5),
    }
}

#[tokio::test]
async fn sql_authorizes_before_paging_completion_and_exact_reads() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = fixture::TestDb::new().await;
    tokio::time::timeout(Duration::from_secs(90), async {
        let spool = tempfile::tempdir().unwrap();
        let service = RecordingService::new(
            db.b.clone(),
            HttpArtifactPlane::new("http://127.0.0.1:1"),
            spool.path().to_owned(),
        )
        .unwrap();
        let reader = identity("recording-query", "reader", &["operations"]);
        let peer = identity("recording-query", "producer", &["operations", "restricted"]);
        let foreign = identity("recording-foreign", "producer", &[]);
        let producer = service.platform_identity(&peer).await.unwrap();
        let other = service.platform_identity(&foreign).await.unwrap();
        let dataset =
            db.a.ensure_recording_dataset(RecordingDatasetDraft::installation_default(
                producer.clone(),
                "native-catalog",
            ))
            .await
            .unwrap();
        let dataset_id =
            RecordingDatasetId::from_uuid(record_uuid(&dataset.id, "recording_dataset").unwrap());
        let other_dataset =
            db.a.ensure_recording_dataset(RecordingDatasetDraft::installation_default(
                other.clone(),
                "native-catalog",
            ))
            .await
            .unwrap();
        let other_dataset_id = RecordingDatasetId::from_uuid(
            record_uuid(&other_dataset.id, "recording_dataset").unwrap(),
        );
        let base = Utc::now() - TimeDelta::hours(1);
        let mut expected = Vec::new();
        // More than the former cap, with equal timestamps crossing page boundaries.
        for i in 0..502 {
            let row =
                db.a.create_recording(RecordingDraft {
                    identity: producer.clone(),
                    authority: veoveo_recording_hub::invocation_authority_record(&peer.authority),
                    dataset_id,
                    application_id: dataset_id.to_string(),
                    recording_key: if i == 0 {
                        "Needle-of-Oldest".into()
                    } else {
                        format!("visible-{i}")
                    },
                    classification: "unclassified".into(),
                    labels: if i % 2 == 0 {
                        vec!["operations".into()]
                    } else {
                        Vec::new()
                    },
                    metadata: BTreeMap::new(),
                    started_at: base + TimeDelta::seconds(i / 37),
                })
                .await
                .unwrap();
            expected.push(RecordingId::from_uuid(
                record_uuid(&row.id, "recording").unwrap(),
            ));
        }
        let oldest = expected[0];
        let mut hidden = None;
        // These sort ahead of every allowed record. A post-query filter would return
        // an empty first page and incorrectly claim the collection is exhausted.
        for i in 0..110 {
            let row =
                db.a.create_recording(RecordingDraft {
                    identity: producer.clone(),
                    authority: veoveo_recording_hub::invocation_authority_record(&peer.authority),
                    dataset_id,
                    application_id: dataset_id.to_string(),
                    recording_key: format!("hidden-{i}"),
                    classification: "restricted".into(),
                    labels: vec!["operations".into(), "restricted".into()],
                    metadata: BTreeMap::new(),
                    started_at: base + TimeDelta::seconds(100),
                })
                .await
                .unwrap();
            hidden = Some(RecordingId::from_uuid(
                record_uuid(&row.id, "recording").unwrap(),
            ));
        }
        let foreign_row =
            db.a.create_recording(RecordingDraft {
                identity: other.clone(),
                authority: veoveo_recording_hub::invocation_authority_record(&foreign.authority),
                dataset_id: other_dataset_id,
                application_id: other_dataset_id.to_string(),
                recording_key: "Foreign-Needle".into(),
                classification: "unclassified".into(),
                labels: Vec::new(),
                metadata: BTreeMap::new(),
                started_at: base + TimeDelta::seconds(101),
            })
            .await
            .unwrap();
        let foreign_id = RecordingId::from_uuid(record_uuid(&foreign_row.id, "recording").unwrap());
        assert!(
            service
                .visible_recording(&reader, hidden.unwrap())
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            service
                .visible_recording(&reader, foreign_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            service
                .visible_recording(&reader, oldest)
                .await
                .unwrap()
                .is_some()
        );
        let uncleared = identity("recording-query", "reader", &[]);
        assert!(
            service
                .visible_recording(&uncleared, oldest)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            service
                .visible_recording(&uncleared, expected[1])
                .await
                .unwrap()
                .is_some()
        );

        // A seal grant does not widen SQL tenant/label visibility or bypass lifecycle checks.
        let mut sealer = reader.clone();
        sealer
            .actor
            .scopes
            .insert(veoveo_types::ScopeName::new("admin:manage").unwrap());
        assert_eq!(
            service.seal(&sealer, oldest).await.unwrap_err().to_string(),
            "Missing Recording scope `recording:seal`."
        );
        sealer.actor.scopes.insert(RecordingScope::Seal.into());
        for id in [hidden.unwrap(), foreign_id] {
            assert_eq!(
                service.seal(&sealer, id).await.unwrap_err().to_string(),
                "recording not found"
            );
        }
        assert_eq!(
            service.seal(&sealer, oldest).await.unwrap_err().to_string(),
            "recording is not sealable from state live"
        );

        let mut visited = Vec::new();
        let mut after = None;
        let mut page_count = 0;
        loop {
            let page = service.catalog_page(&reader, after.as_ref()).await.unwrap();
            page_count += 1;
            assert_eq!(page.limit, 100);
            assert_eq!(page.items.len(), if page_count <= 5 { 100 } else { 2 });
            assert!(
                page.items
                    .iter()
                    .all(|item| item.layer_count == 0 && item.committed_layer_count == 0)
            );
            visited.extend(
                page.items
                    .iter()
                    .map(|item| RecordingId::from_uuid(item.recording_id.as_uuid())),
            );
            let Some(cursor) = page.next_cursor else {
                break;
            };
            let uri = RecordingResource::Catalog(Some(cursor)).to_uri().unwrap();
            let RecordingResource::Catalog(cursor) =
                RecordingResource::parse(uri.as_str()).unwrap()
            else {
                panic!("catalog address");
            };
            after = cursor;
            assert!(page_count < 6, "catalog cursor did not make progress");
        }
        expected.reverse();
        assert_eq!(
            visited, expected,
            "all authorized recordings appear exactly once, newest first"
        );
        assert_eq!(page_count, 6);

        assert_eq!(
            service
                .complete_recording_ids(&reader, "needle-OF-oldest")
                .await
                .unwrap(),
            [oldest.to_string()]
        );
        assert_eq!(
            service
                .complete_recording_ids(&reader, &oldest.to_string().to_uppercase())
                .await
                .unwrap(),
            [oldest.to_string()]
        );
        assert_eq!(
            service
                .complete_recording_ids(&reader, "")
                .await
                .unwrap()
                .len(),
            101
        );
        for needle in ["hidden", "Foreign-Needle", "' OR true --"] {
            assert!(
                service
                    .complete_recording_ids(&reader, needle)
                    .await
                    .unwrap()
                    .is_empty()
            );
        }
        assert!(
            service
                .complete_recording_ids(&reader, &"a".repeat(513))
                .await
                .is_err()
        );
        assert!(
            service
                .complete_recording_ids(&uncleared, "Needle-of-Oldest")
                .await
                .unwrap()
                .is_empty()
        );
        let uncleared_page = service.catalog_page(&uncleared, None).await.unwrap();
        assert_eq!(uncleared_page.items.len(), 100);
        assert!(
            uncleared_page
                .items
                .iter()
                .all(|item| item.labels.is_empty())
        );
        let foreign_page = service.catalog_page(&foreign, None).await.unwrap();
        assert_eq!(foreign_page.items.len(), 1);
        assert_eq!(
            foreign_page.items[0].recording_id.as_uuid(),
            foreign_id.as_uuid()
        );
        let scope = RecordingReadScope {
            tenant_id: producer.tenant_id,
            data_labels: vec!["operations".into()],
        };
        for limit in [0, 102] {
            assert!(db.a.list_recordings(&scope, None, limit).await.is_err());
            assert!(
                db.a.complete_recording_ids(&scope, "", limit)
                    .await
                    .is_err()
            );
        }

        grants::qualify(&service, &reader, dataset_id, oldest).await;

        let first =
            db.a.open_recording_layer(
                RecordingLayerDraft::capture(
                    producer.clone(),
                    oldest,
                    0,
                    "native/first.rrd".into(),
                    Some(base),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        db.a.open_recording_layer(
            RecordingLayerDraft::capture(
                producer.clone(),
                oldest,
                1,
                "native/second.rrd".into(),
                Some(base),
            )
            .unwrap(),
        )
        .await
        .unwrap();
        // Seed a committed layer state: count qualification never reads a layer payload.
        db.a.client()
            .query("UPDATE ONLY $layer SET state = 'committed';")
            .bind(("layer", first.id))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            db.a.recording_layer_counts(producer.tenant_id, oldest)
                .await
                .unwrap(),
            RecordingLayerCounts {
                total: 2,
                committed: 1
            }
        );
        assert_eq!(
            db.a.recording_layer_counts(other.tenant_id, oldest)
                .await
                .unwrap(),
            RecordingLayerCounts::default()
        );
        let view = service
            .recording_view(&reader, oldest)
            .await
            .unwrap()
            .unwrap();
        assert_eq!((view.layer_count, view.committed_layer_count), (2, 1));
    })
    .await
    .expect("Recording catalog qualification exceeded 90 seconds");
}
