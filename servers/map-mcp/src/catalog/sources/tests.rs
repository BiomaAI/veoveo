use super::*;
use crate::contract::*;
use chrono::{DateTime, Utc};
use std::{collections::BTreeSet, time::Duration};
use veoveo_platform_store::{PlatformStore, PrincipalKind};

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
        credential: Some(SourceCredential::Bearer {
            secret_ref: SecretReference::parse("private-bearer-key").unwrap(),
        }),
        publisher_key_refs: BTreeSet::from([
            SecretReference::parse("private-publisher-key").unwrap()
        ]),
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

#[tokio::test]
async fn source_pages_and_completion_select_tenant_before_limits_and_recheck_access() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::new().await;
        let writer = MapCatalog::new(db.a.clone());
        let reader = MapCatalog::new(db.b.clone());
        let owner = scope(&db.a, "sources", "author").await;
        let peer = scope(&db.a, "sources", "peer").await;
        let foreign = scope(&db.a, "foreign", "author").await;
        let at = Utc::now();
        for n in 0..110 {
            let record = source(n, at);
            writer
                .create_source(&foreign, record.clone())
                .await
                .unwrap();
            db.a.client()
                .query("UPDATE ONLY $id SET canonical_json = '{' RETURN NONE;")
                .bind(("id", RecordId::new("map_source", record.source_id.as_str())))
                .await
                .unwrap()
                .check()
                .unwrap();
        }
        for n in 1000..1125 {
            writer.create_source(&owner, source(n, at)).await.unwrap();
        }
        let first = reader
            .sources_page(&peer, &MapSourcesUri::new(None))
            .await
            .unwrap();
        assert_eq!(first.items().len(), 100);
        assert_eq!(first.items()[0].source_id(), &source(1000, at).source_id);
        let continuation = MapSourcesUri::new(first.next_cursor().cloned());
        assert!(
            reader
                .sources_page(&foreign, &continuation)
                .await
                .unwrap()
                .items()
                .is_empty()
        );
        let last = reader.sources_page(&peer, &continuation).await.unwrap();
        assert_eq!(last.items().len(), 25);
        assert!(last.next_cursor().is_none());
        let selected = source(1124, at);
        assert!(
            reader
                .source(&foreign, &selected.source_id)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            reader.source(&peer, &selected.source_id).await.unwrap(),
            Some(selected.clone())
        );
        assert_eq!(reader.complete_sources(&peer, "").await.unwrap().len(), 101);
        assert_eq!(
            reader
                .complete_sources(&peer, &selected.source_id.to_string().to_uppercase())
                .await
                .unwrap(),
            vec![selected.source_id.clone()]
        );
        assert!(
            reader
                .complete_sources(&peer, "' OR true --")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(reader.complete_sources(&peer, "\n").await.is_err());
        assert!(
            reader
                .complete_sources(&peer, &"x".repeat(513))
                .await
                .is_err()
        );
        let json = serde_json::to_string(&first).unwrap();
        for private in [
            "location",
            "credential",
            "publisher_key_refs",
            "private-bearer-key",
            "private-publisher-key",
            "maximum_download_bytes",
            "expected_media_types",
        ] {
            assert!(!json.contains(private), "{private}");
        }
        // Replacement keeps timestamps in their separate document/Store clocks.
        let mut replaced = selected.clone();
        replaced.enabled = false;
        replaced.record_version = 2;
        replaced.updated_at = Utc::now();
        writer
            .replace_source(&owner, replaced.clone(), 1)
            .await
            .unwrap();
        assert_eq!(
            reader.source(&peer, &selected.source_id).await.unwrap(),
            Some(replaced)
        );
        assert_eq!(
            reader
                .sources_page(&peer, &continuation)
                .await
                .unwrap()
                .items()
                .len(),
            25
        );
        // A cursor grants no authority when a record moves to a different tenant.
        db.a.client()
            .query("UPDATE ONLY $id SET tenant = $tenant RETURN NONE;")
            .bind((
                "id",
                RecordId::new("map_source", selected.source_id.as_str()),
            ))
            .bind(("tenant", foreign.identity.tenant_id.record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            reader
                .sources_page(&peer, &continuation)
                .await
                .unwrap()
                .items()
                .len(),
            24
        );
        assert!(
            reader
                .source(&peer, &selected.source_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            reader
                .complete_sources(&peer, selected.source_id.as_str())
                .await
                .unwrap()
                .is_empty()
        );
    })
    .await
    .expect("source catalog fixture deadline");
}

#[tokio::test]
async fn selected_source_documents_must_agree_with_indexed_metadata() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::new().await;
        let catalog = MapCatalog::new(db.a.clone());
        let access = scope(&db.a, "source-corruption", "author").await;
        let record = source(1, Utc::now());
        catalog
            .create_source(&access, record.clone())
            .await
            .unwrap();
        let wire = serde_json::to_value(&record).unwrap();
        for (field, value) in [
            ("source_id", serde_json::json!(MapSourceId::new())),
            ("dataset_id", serde_json::json!(MapDatasetId::new())),
            ("name", serde_json::json!("different")),
            ("adapter_kind", serde_json::json!("open_street_map")),
            ("authority", serde_json::json!("community")),
            ("map_families", serde_json::json!(["maritime"])),
            ("enabled", serde_json::json!(false)),
            ("record_version", serde_json::json!(2)),
            ("maximum_download_bytes", serde_json::json!(0)),
            ("updated_at", serde_json::json!("2020-01-01T00:00:00Z")),
        ] {
            let mut bad = wire.clone();
            bad[field] = value;
            db.a.client()
                .query("UPDATE ONLY $id SET canonical_json = $json RETURN NONE;")
                .bind(("id", RecordId::new("map_source", record.source_id.as_str())))
                .bind(("json", serde_json::to_string(&bad).unwrap()))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(
                catalog.source(&access, &record.source_id).await.is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .sources_page(&access, &MapSourcesUri::new(None))
                    .await
                    .is_err(),
                "{field}"
            );
        }
        // A matching key/document cannot hide the wrong physical record identity.
        db.a.client()
            .query(
                "BEGIN; LET $content = (SELECT * OMIT id FROM ONLY $id); DELETE $id;
             CREATE $wrong CONTENT $content;
             UPDATE ONLY $wrong SET canonical_json = $json RETURN NONE; COMMIT;",
            )
            .bind(("id", RecordId::new("map_source", record.source_id.as_str())))
            .bind((
                "wrong",
                RecordId::new("map_source", MapSourceId::new().as_str()),
            ))
            .bind(("json", serde_json::to_string(&wire).unwrap()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(catalog.source(&access, &record.source_id).await.is_err());
        assert!(
            catalog
                .sources_page(&access, &MapSourcesUri::new(None))
                .await
                .is_err()
        );
    })
    .await
    .expect("source corruption fixture deadline");
}
