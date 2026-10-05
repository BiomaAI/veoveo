use super::*;
use std::time::Duration;
use veoveo_platform_store::{PlatformStore, PrincipalKind};

fn id(n: usize) -> MobilityProfileId {
    format!("mobility-{n:08x}-0000-7000-8000-000000000000")
        .parse()
        .unwrap()
}
fn version(n: u64) -> MobilityProfileVersion {
    MobilityProfileVersion::new(n).unwrap()
}
fn profile(n: usize, v: u64) -> MobilityProfile {
    let mut wire: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/mobility.json")).unwrap();
    wire["profile"]["metadata"]["profile_id"] = serde_json::json!(id(n));
    wire["profile"]["metadata"]["version"] = serde_json::json!(v);
    if v == 1 {
        wire["profile"]["metadata"]["valid_until"] = serde_json::json!("2026-01-02T00:00:00Z");
    }
    serde_json::from_value(wire).unwrap()
}
fn record(n: usize, v: u64) -> RecordId {
    RecordId::new("map_mobility_profile", format!("{}:{v}", id(n)))
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

#[tokio::test]
async fn pages_and_completions_select_tenant_and_parent_before_numeric_version_limits() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::with_modules(vec![crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap()).unwrap()]).await;
        let writer = MapCatalog::new(db.a.clone());
        let reader = MapCatalog::new(db.b.clone());
        let owner = scope(&db.a, "mobility", "author").await;
        let peer = scope(&db.a, "mobility", "peer").await;
        let foreign = scope(&db.a, "foreign", "author").await;
        for n in 0..110 {
            writer
                .create_mobility_profile(&foreign, profile(n, 1))
                .await
                .unwrap();
            db.a.client()
                .query(include_str!("../../queries/catalog/mobility/tests/pages_and_completions_select_tenant_and_parent_before_numeric_version_limits/statement_1.surql"))
                .bind(("id", record(n, 1)))
                .await
                .unwrap()
                .check()
                .unwrap();
        }
        for v in 1..=125 {
            writer
                .create_mobility_profile(&owner, profile(1000, v))
                .await
                .unwrap();
        }
        for n in 1001..=1105 {
            writer
                .create_mobility_profile(&owner, profile(n, 1))
                .await
                .unwrap();
        }
        let first = reader
            .mobility_profiles_page(&peer, &MapMobilityProfilesUri::new(None))
            .await
            .unwrap();
        assert_eq!(first.items().len(), 100);
        assert_eq!(first.items()[0], profile(1000, 1));
        assert_eq!(first.items()[9].metadata().version, version(10));
        let continuation = MapMobilityProfilesUri::new(first.next_cursor().cloned());
        assert!(
            reader
                .mobility_profiles_page(&foreign, &continuation)
                .await
                .unwrap()
                .items()
                .is_empty()
        );
        let second = reader
            .mobility_profiles_page(&peer, &continuation)
            .await
            .unwrap();
        assert_eq!(second.items().len(), 100);
        assert_eq!(second.items()[0], profile(1000, 101));
        assert_eq!(second.items()[25], profile(1001, 1));
        let last = reader
            .mobility_profiles_page(
                &peer,
                &MapMobilityProfilesUri::new(second.next_cursor().cloned()),
            )
            .await
            .unwrap();
        assert_eq!(last.items().len(), 30);
        assert!(last.next_cursor().is_none());
        assert_eq!(last.items()[29], profile(1105, 1));
        assert_eq!(
            reader
                .mobility_profile(&peer, &id(1000), version(1))
                .await
                .unwrap(),
            Some(profile(1000, 1))
        );
        assert!(
            reader
                .mobility_profile(&foreign, &id(1000), version(1))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            reader
                .mobility_profile(&peer, &id(1000), version(126))
                .await
                .unwrap()
                .is_none()
        );
        let keys = reader.complete_mobility_profiles(&peer, "").await.unwrap();
        assert_eq!(keys.len(), 101);
        assert_eq!(keys[0], id(1000));
        assert_eq!(keys[100], id(1100));
        assert_eq!(
            reader
                .complete_mobility_profiles(&peer, &id(1105).to_string().to_uppercase())
                .await
                .unwrap(),
            vec![id(1105)]
        );
        assert_eq!(
            reader
                .complete_mobility_versions(&peer, Some(&id(1000)), "")
                .await
                .unwrap(),
            (1..=101).map(version).collect::<Vec<_>>()
        );
        assert_eq!(
            reader
                .complete_mobility_versions(&peer, None, "")
                .await
                .unwrap(),
            (1..=101).map(version).collect::<Vec<_>>()
        );
        assert_eq!(
            reader
                .complete_mobility_versions(&peer, Some(&id(1105)), "")
                .await
                .unwrap(),
            vec![version(1)]
        );
        assert_eq!(
            reader
                .complete_mobility_versions(&peer, Some(&id(1000)), "25")
                .await
                .unwrap(),
            vec![version(25), version(125)]
        );
        assert!(
            reader
                .complete_mobility_versions(&foreign, Some(&id(1000)), "")
                .await
                .unwrap()
                .is_empty()
        );
        for needle in ["\n".to_owned(), "x".repeat(513)] {
            assert!(
                reader
                    .complete_mobility_profiles(&peer, &needle)
                    .await
                    .is_err()
            );
            assert!(
                reader
                    .complete_mobility_versions(&peer, None, &needle)
                    .await
                    .is_err()
            );
        }
        assert!(
            reader
                .complete_mobility_profiles(&peer, "' OR true --")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            reader
                .complete_mobility_versions(&peer, None, "' OR true --")
                .await
                .unwrap()
                .is_empty()
        );
        db.a.client()
            .query(include_str!("../../queries/catalog/mobility/tests/pages_and_completions_select_tenant_and_parent_before_numeric_version_limits/statement_2.surql"))
            .bind(("id", record(1000, 101)))
            .bind(("tenant", foreign.identity.tenant_id.record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let changed = reader
            .mobility_profiles_page(&peer, &continuation)
            .await
            .unwrap();
        assert_eq!(changed.items()[0], profile(1000, 102));
        assert!(
            reader
                .mobility_profile(&peer, &id(1000), version(101))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            reader
                .complete_mobility_versions(&peer, Some(&id(1000)), "101")
                .await
                .unwrap()
                .is_empty()
        );
    })
    .await
    .expect("mobility catalog fixture deadline");
}

#[tokio::test]
async fn selected_profile_metadata_and_physical_identity_must_agree() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::with_modules(vec![crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap()).unwrap()]).await;
        let catalog = MapCatalog::new(db.a.clone());
        let access = scope(&db.a,"mobility-corruption","author").await;
        let profile = profile(1,1);
        catalog.create_mobility_profile(&access,profile.clone()).await.unwrap();
        let wire = serde_json::to_value(&profile).unwrap();
        for (path, value) in [
            ("/profile/metadata/profile_id",serde_json::json!(id(2))),
            ("/profile/metadata/version",serde_json::json!(2)),
            ("/profile/metadata/name",serde_json::json!("different")),
            ("/profile/metadata/valid_from",serde_json::json!("2025-01-01T00:00:00Z")),
            ("/profile/metadata/valid_until",serde_json::json!("2026-01-03T00:00:00Z")),
            ("/profile/preferred_speed",serde_json::json!(30)),
            ("/profile/planning/maximum_route_points",serde_json::json!(1)),
            ("/profile/metadata/version",serde_json::json!(0)),
            ("/profile/metadata/version",serde_json::json!(u64::MAX)),
        ] {
            let mut bad = wire.clone(); *bad.pointer_mut(path).unwrap() = value;
            db.a.client().query(include_str!("../../queries/catalog/mobility/tests/selected_profile_metadata_and_physical_identity_must_agree/statement_1.surql"))
                .bind(("id",record(1,1))).bind(("json",serde_json::to_string(&bad).unwrap()))
                .await.unwrap().check().unwrap();
            assert!(catalog.mobility_profile(&access,&id(1),version(1)).await.is_err(),"{path}");
            assert!(catalog.mobility_profiles_page(&access,&MapMobilityProfilesUri::new(None)).await.is_err(),"{path}");
        }
        db.a.client().query(include_str!("../../queries/catalog/mobility/tests/selected_profile_metadata_and_physical_identity_must_agree/statement_2.surql"))
            .bind(("id",record(1,1))).bind(("json",serde_json::to_string(&wire).unwrap()))
            .await.unwrap().check().unwrap();
        assert!(catalog.mobility_profile(&access,&id(1),version(1)).await.is_err());
        db.a.client().query(include_str!("../../queries/catalog/mobility/tests/selected_profile_metadata_and_physical_identity_must_agree/statement_3.surql"))
            .bind(("id",record(1,1))).bind(("wrong",record(2,1))).await.unwrap().check().unwrap();
        assert!(catalog.mobility_profile(&access,&id(1),version(1)).await.is_err());
        assert!(catalog.mobility_profiles_page(&access,&MapMobilityProfilesUri::new(None)).await.is_err());
    }).await.expect("mobility corruption fixture deadline");
}
