use super::*;
use crate::contract::*;
use std::time::Duration;
use veoveo_platform_store::{PlatformStore, PrincipalKind, RecordId};

fn key(prefix: &str, n: usize) -> String {
    format!("{prefix}-{n:08x}-0000-7000-8000-000000000000")
}
async fn scope(store: &PlatformStore, tenant: &str, principal: &str) -> MapAccessContext {
    MapAccessContext {
        identity: store
            .ensure_identity(
                tenant,
                principal,
                "https://fixture.local",
                principal,
                PrincipalKind::Service,
            )
            .await
            .unwrap(),
    }
}
fn license() -> DatasetLicense {
    DatasetLicense {
        license_id: "fixture".into(),
        source_terms_uri: HttpsEndpoint::parse("https://fixture.local/terms").unwrap(),
        attribution: "Fixture".into(),
        redistribution_allowed: true,
        derivatives_allowed: true,
        offline_bundle_allowed: true,
        expires_at: None,
    }
}
fn source(n: usize, at: DateTime<Utc>) -> RegisteredSource {
    RegisteredSource {
        source_id: key("source", n).parse().unwrap(),
        dataset_id: key("dataset", n).parse().unwrap(),
        name: format!("source-{n}"),
        adapter_kind: SourceAdapterKind::AuthorityVector,
        authority: AuthorityClass::SyntheticTest,
        acquisition_model: AcquisitionModel::Snapshot,
        map_families: BTreeSet::from([MapFamily::RoadStreet, MapFamily::Intermodal]),
        location: SourceLocation::Https {
            endpoint: HttpsEndpoint::parse("https://fixture.local/source").unwrap(),
            allowed_redirect_hosts: BTreeSet::new(),
        },
        credential: None,
        publisher_key_refs: BTreeSet::new(),
        expected_media_types: BTreeSet::from(["application/json".into()]),
        maximum_download_bytes: 1024,
        maximum_elapsed_seconds: 30,
        license: license(),
        enabled: true,
        record_version: 1,
        created_at: at,
        updated_at: at,
    }
}
fn release(n: usize, source: &RegisteredSource, at: DateTime<Utc>) -> DatasetRelease {
    DatasetRelease {
        release_id: key("release", n).parse().unwrap(),
        dataset_id: source.dataset_id.clone(),
        source_id: source.source_id.clone(),
        version_label: "fixture".into(),
        source_digest_sha256: "a".repeat(64),
        coverage: Wgs84BoundingBox {
            west: -1.,
            south: -1.,
            east: 1.,
            north: 1.,
        },
        acquired_at: at,
        valid_from: at,
        valid_until: None,
        schema_version: 1,
        normalization_pipeline_version: "fixture".into(),
        routing_build_version: None,
        license: license(),
        raw_artifact_uri: "artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4471"
            .parse()
            .unwrap(),
        normalized_artifact_uris: vec![
            "artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4472"
                .parse()
                .unwrap(),
        ],
        quality_report_uri: "artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4473"
            .parse()
            .unwrap(),
        supersedes_release_id: None,
        state: DatasetReleaseState::Staged,
        record_version: 1,
        updated_at: at,
    }
}
async fn fixture(
    catalog: &MapCatalog,
    scope: &MapAccessContext,
    n: usize,
    at: DateTime<Utc>,
) -> (RegisteredSource, DatasetRelease) {
    let source = catalog.create_source(scope, source(n, at)).await.unwrap();
    let release = catalog
        .create_release(scope, release(n, &source, at))
        .await
        .unwrap();
    let release = catalog
        .activate_release(scope, release, None)
        .await
        .unwrap();
    (source, release)
}
fn pointer(scope: &MapAccessContext, n: usize) -> RecordId {
    RecordId::new(
        "map_active_release",
        format!("{}:{}", scope.identity.tenant_id, key("dataset", n)),
    )
}
async fn mutate(store: &PlatformStore, record: RecordId, fields: &str) {
    store
        .client()
        .query(format!("UPDATE ONLY $record SET {fields} RETURN NONE;"))
        .bind(("record", record))
        .await
        .unwrap()
        .check()
        .unwrap();
}

#[tokio::test]
async fn authority_selects_complete_current_tenant_release_and_family_sets() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::new().await;
        let writer = MapCatalog::new(db.a.clone());
        let reader = MapCatalog::new(db.b.clone());
        let owner = scope(&db.a, "routing", "author").await;
        let peer = scope(&db.a, "routing", "peer").await;
        let foreign = scope(&db.a, "foreign", "author").await;
        let at = Utc::now();
        let families = BTreeSet::from([MapFamily::RoadStreet]);
        for n in 0..110 {
            fixture(&writer, &foreign, n, at).await;
            mutate(
                &db.a,
                RecordId::new("map_dataset_release", key("release", n)),
                "canonical_json = '{'",
            )
            .await;
            mutate(
                &db.a,
                RecordId::new("map_source", key("source", n)),
                "canonical_json = '{'",
            )
            .await;
        }
        for n in 1000..1125 {
            fixture(&writer, &owner, n, at).await;
        }
        let (ids, selected) = reader
            .routing_authority(&peer, &families, at)
            .await
            .unwrap()
            .into_parts();
        assert_eq!(
            ids,
            (1000..1125)
                .map(|n| key("release", n).parse().unwrap())
                .collect()
        );
        assert_eq!(selected, families);
        assert!(
            reader
                .routing_authority(&peer, &BTreeSet::new(), at)
                .await
                .unwrap()
                .into_parts()
                .0
                .is_empty()
        );
        assert!(
            reader
                .routing_authority(&peer, &families, at - chrono::Duration::seconds(1))
                .await
                .unwrap()
                .into_parts()
                .0
                .is_empty()
        );
        // Activation changes the selected release, even though the superseded
        // row still has an active state; the pointer determines membership.
        let source = source(1000, at);
        let next = writer
            .create_release(&owner, release(2000, &source, at))
            .await
            .unwrap();
        writer
            .activate_release(&owner, next, Some(1))
            .await
            .unwrap();
        let current = reader
            .routing_authority(&peer, &families, at)
            .await
            .unwrap()
            .into_parts()
            .0;
        assert_eq!(current.len(), 125);
        assert!(!current.contains(&key("release", 1000).parse().unwrap()));
        assert!(current.contains(&key("release", 2000).parse().unwrap()));
        mutate(
            &db.a,
            RecordId::new("map_source", source.source_id.as_str()),
            "enabled = false, canonical_json = '{'",
        )
        .await;
        assert_eq!(
            reader
                .routing_authority(&peer, &families, at)
                .await
                .unwrap()
                .into_parts()
                .0
                .len(),
            124
        );
        db.a.client()
            .query("UPDATE ONLY $release SET valid_until = $at, canonical_json = '{' RETURN NONE;")
            .bind((
                "release",
                RecordId::new("map_dataset_release", key("release", 1001)),
            ))
            .bind(("at", at))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            reader
                .routing_authority(&peer, &families, at)
                .await
                .unwrap()
                .into_parts()
                .0
                .len(),
            123
        );
    })
    .await
    .expect("routing authority selection exceeded 90 seconds");
}

#[tokio::test]
async fn mismatched_parents_tenants_and_lifecycle_are_excluded_before_document_decode() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::new().await;
        let writer = MapCatalog::new(db.a.clone());
        let reader = MapCatalog::new(db.b.clone());
        let owner = scope(&db.a, "relationships", "author").await;
        let at = Utc::now();
        let families = BTreeSet::from([MapFamily::RoadStreet]);
        for (n, (target, fields)) in [
            ("pointer", "tenant = tenant:other"),
            (
                "pointer",
                "dataset_key = 'dataset-ffffffff-0000-7000-8000-000000000000'",
            ),
            (
                "pointer",
                "release_key = 'release-ffffffff-0000-7000-8000-000000000000'",
            ),
            ("release", "tenant = tenant:other"),
            (
                "release",
                "dataset_key = 'dataset-ffffffff-0000-7000-8000-000000000000'",
            ),
            (
                "release",
                "source_key = 'source-ffffffff-0000-7000-8000-000000000000'",
            ),
            (
                "release",
                "release_key = 'release-ffffffff-0000-7000-8000-000000000000'",
            ),
            ("release", "state = 'staged'"),
            ("release", "state = 'retired'"),
            ("release", "state = 'quarantined'"),
            ("release", "valid_from = d'2100-01-01T00:00:00Z'"),
            ("release", "valid_until = d'2000-01-01T00:00:00Z'"),
            ("source", "tenant = tenant:other"),
            (
                "source",
                "dataset_key = 'dataset-ffffffff-0000-7000-8000-000000000000'",
            ),
            (
                "source",
                "source_key = 'source-ffffffff-0000-7000-8000-000000000000'",
            ),
            ("source", "enabled = false"),
            ("source", "map_families = ['rail_transit']"),
        ]
        .into_iter()
        .enumerate()
        {
            fixture(&writer, &owner, n, at).await;
            let record = match target {
                "pointer" => pointer(&owner, n),
                "release" => RecordId::new("map_dataset_release", key("release", n)),
                "source" => RecordId::new("map_source", key("source", n)),
                _ => unreachable!(),
            };
            mutate(&db.a, record, fields).await;
            mutate(
                &db.a,
                RecordId::new("map_dataset_release", key("release", n)),
                "canonical_json = '{'",
            )
            .await;
            mutate(
                &db.a,
                RecordId::new("map_source", key("source", n)),
                "canonical_json = '{'",
            )
            .await;
            assert!(
                reader
                    .routing_authority(&owner, &families, at)
                    .await
                    .unwrap()
                    .into_parts()
                    .0
                    .is_empty(),
                "{target} {fields}"
            );
        }
    })
    .await
    .expect("routing authority relationship qualification exceeded 90 seconds");
}

#[tokio::test]
async fn matching_retained_documents_must_agree_with_selection_fields() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::new().await;
        let writer = MapCatalog::new(db.a.clone());
        let reader = MapCatalog::new(db.b.clone());
        let owner = scope(&db.a, "documents", "author").await;
        let at = Utc::now();
        let families = BTreeSet::from([MapFamily::RoadStreet]);
        for (n, (target, field, value)) in [
            ("source", "source_id", serde_json::json!(key("source", 999))),
            (
                "source",
                "dataset_id",
                serde_json::json!(key("dataset", 999)),
            ),
            ("source", "enabled", serde_json::json!(false)),
            (
                "source",
                "map_families",
                serde_json::json!(["rail_transit"]),
            ),
            ("source", "record_version", serde_json::json!(2)),
            (
                "release",
                "release_id",
                serde_json::json!(key("release", 999)),
            ),
            (
                "release",
                "source_id",
                serde_json::json!(key("source", 999)),
            ),
            (
                "release",
                "dataset_id",
                serde_json::json!(key("dataset", 999)),
            ),
            ("release", "state", serde_json::json!("quarantined")),
            (
                "release",
                "valid_from",
                serde_json::json!("2000-01-01T00:00:00Z"),
            ),
            (
                "release",
                "valid_until",
                serde_json::json!("2100-01-01T00:00:00Z"),
            ),
            ("release", "record_version", serde_json::json!(3)),
        ]
        .into_iter()
        .enumerate()
        {
            let (source, release) = fixture(&writer, &owner, n, at).await;
            let (record, mut document) = if target == "source" {
                (
                    RecordId::new("map_source", source.source_id.as_str()),
                    serde_json::to_value(source).unwrap(),
                )
            } else {
                (
                    RecordId::new("map_dataset_release", release.release_id.as_str()),
                    serde_json::to_value(release).unwrap(),
                )
            };
            document[field] = value;
            db.a.client()
                .query("UPDATE ONLY $record SET canonical_json = $json RETURN NONE;")
                .bind(("record", record))
                .bind(("json", serde_json::to_string(&document).unwrap()))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(
                reader
                    .routing_authority(&owner, &families, at)
                    .await
                    .is_err(),
                "{target}.{field}"
            );
            db.a.client()
                .query("DELETE ONLY $pointer;")
                .bind(("pointer", pointer(&owner, n)))
                .await
                .unwrap()
                .check()
                .unwrap();
        }
        let (_, release) = fixture(&writer, &owner, 100, at).await;
        mutate(&db.a, pointer(&owner, 100), "record_version = 0").await;
        assert!(reader.routing_authority(&owner, &families, at).await.is_err());
        db.a.client().query("BEGIN TRANSACTION; DELETE ONLY $pointer; CREATE ONLY $alias CONTENT { tenant: $tenant, dataset_key: $dataset, release_key: $release, activated_by: $owner, activated_at: $at, record_version: 1 }; COMMIT TRANSACTION;")
            .bind(("pointer", pointer(&owner,100)))
            .bind(("alias",RecordId::new("map_active_release","wrong-identity")))
            .bind(("tenant",owner.identity.tenant_id.record_id()))
            .bind(("dataset",release.dataset_id.to_string()))
            .bind(("release",release.release_id.to_string()))
            .bind(("owner",owner.identity.principal_id.record_id()))
            .bind(("at",at)).await.unwrap().check().unwrap();
        assert!(reader.routing_authority(&owner, &families, at).await.is_err());
    })
    .await
    .expect("routing authority retained-document qualification exceeded 90 seconds");
}
