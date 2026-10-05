use super::*;

use crate::{analytics::MapAnalyticsConfig, authoring::AuthoringService};
use std::{collections::BTreeSet, time::Duration};
use veoveo_mcp_contract::{Principal, PrincipalKind};
use veoveo_mcp_knowledge_extension::ReadPolicy;
use veoveo_types::*;

#[test]
fn knowledge_revision_covers_modification_time_and_actor() {
    let time = Utc::now();
    let mut member = ObservedMap {
        address: MapKnowledgeMember::Location {
            location: LocationId::from_stable_key(b"revision-fixture"),
        },
        title: "A location".into(),
        text: "location summary".into(),
        access: None,
        modified_at: Some(time),
        modified_by: None,
    };
    let (text, original) = member.document().unwrap();
    assert_eq!(original.revision(), member.document().unwrap().1.revision());
    member.modified_at = Some(time + chrono::TimeDelta::seconds(1));
    let (same_text, modified) = member.document().unwrap();
    assert_eq!(text, same_text);
    assert_ne!(original.revision(), modified.revision());
    member.modified_by = Some(ModifiedBy::Principal("modifier".parse().unwrap()));
    let (_, attributed) = member.document().unwrap();
    assert_ne!(modified.revision(), attributed.revision());
}
fn identity(tenant: &str, context: &str) -> GatewayInternalIdentity {
    let now = Utc::now();
    let actor = Principal {
        id: "author".parse().unwrap(),
        kind: PrincipalKind::User,
        issuer: "https://identity.test".parse().unwrap(),
        subject: "author".parse().unwrap(),
        tenant: Some(tenant.parse().unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: BTreeSet::from([MapScope::FeatureRead.into(), MapScope::DatasetRead.into()]),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(now),
    };
    GatewayInternalIdentity {
        issuer: "veoveo-internal".parse().unwrap(),
        profile: "operator".parse().unwrap(),
        server: "map".parse().unwrap(),
        authority: InvocationAuthority {
            tenant: tenant.parse().unwrap(),
            work_context: context.parse().unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: "r1".parse().unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(actor.id.clone()),
                initial_grants: vec![],
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Direct {
                initiator: actor.id.clone(),
            },
        },
        actor,
        request_context: None,
        jwt_id: "fixture".parse().unwrap(),
        issued_at: now,
        not_before: now,
        expires_at: now + chrono::TimeDelta::hours(1),
    }
}
fn analytics(root: &tempfile::TempDir) -> MapAnalytics {
    MapAnalytics::open(MapAnalyticsConfig {
        database_path: root.path().join("map.duckdb"),
        authoring_task_root: root.path().join("tasks"),
        spill_dir: root.path().join("spill"),
        spatial_extension: std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION")
            .expect("qualified Spatial extension required")
            .into(),
        memory_limit: "256MB".into(),
        threads: 1,
    })
    .unwrap()
}

#[test]
fn declarations_and_cursors_preserve_domain_scope_and_member_identity() {
    let layer = FeatureLayerId::new();
    let members = [
        MapKnowledgeMember::Layer {
            layer: layer.clone(),
        },
        MapKnowledgeMember::Feature {
            layer: layer.clone(),
            feature: MapFeatureId::new(),
        },
        MapKnowledgeMember::Publication {
            layer,
            publication: LayerPublicationId::new(),
        },
        MapKnowledgeMember::Location {
            location: LocationId::new(),
        },
        MapKnowledgeMember::Facility {
            facility: FacilityId::new(),
        },
        MapKnowledgeMember::Release {
            dataset: MapDatasetId::new(),
            release: DatasetReleaseId::new(),
        },
    ];
    for member in members {
        assert_eq!(
            MapKnowledgeMember::parse(member.to_uri().as_str()).unwrap(),
            member
        );
        let collection = member.collection();
        assert_eq!(
            collection.descriptor().required_scopes(),
            &BTreeSet::from([collection.scope().into()])
        );
        let cursor = MapKnowledgeCursor::after(member.clone());
        let page = MapKnowledgePageUri::new(collection)
            .with_cursor(cursor.clone())
            .unwrap();
        assert_eq!(
            MapKnowledgePageUri::parse(page.to_uri().as_str()).unwrap(),
            page
        );
        for other in MapKnowledgeCollection::ALL {
            if other != collection {
                assert!(
                    MapKnowledgePageUri::new(other)
                        .with_cursor(cursor.clone())
                        .is_err()
                );
            }
        }
        assert!(MapKnowledgeMember::parse(&format!("{}?unknown=1", member.to_uri())).is_err());
    }
    assert!(MapKnowledgePageUri::parse("map://knowledge/features?cursor=00").is_err());
    assert!(MapKnowledgePageUri::parse("map://knowledge/features?unknown=1").is_err());
    assert!(MapKnowledgePageUri::parse("map://knowledge/features?").is_err());
}

#[tokio::test]
async fn authoring_knowledge_uses_current_parent_access_before_decoding_and_pages_features() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = crate::test_store::TestDb::with_modules(vec![crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap()).unwrap()]).await;
        let root = tempfile::TempDir::new().unwrap();
        let analytics = analytics(&root);
        let authoring = AuthoringService::new(db.a.clone(), analytics.clone());
        let catalog = MapCatalog::new(db.b.clone());
        let identity = identity("map-knowledge", "mission");
        let scope = MapAccessContext { identity: db.a.ensure_identity("map-knowledge", "author", "https://identity.test", "author", veoveo_platform_store::PrincipalKind::User).await.unwrap() };
        let layer = authoring.create_layer(&identity, &scope, CreateFeatureLayerRequest {
            title: "Inspections".into(), description: None, content_class: FeatureContentClass::Boundaries,
            property_schema: serde_json::json!({"type":"object","properties":{},"additionalProperties":false}), style: None,
        }).await.unwrap();
        let layer_address = MapKnowledgeMember::Layer { layer: layer.layer_id.clone() };
        let first = read(&catalog, &analytics, &identity, &scope, &layer_address).await.unwrap().unwrap();
        let first_revision = first.document().unwrap().1;
        assert!(first_revision.modified_by().is_none());
        assert_eq!(first_revision.access().unwrap().read_policy, ReadPolicy::SelectedWorkContextMembers {});
        assert!(first_revision.access().unwrap().grants.is_empty());
        for (batch, count) in [100, 5].into_iter().enumerate() {
            let mutations = (0..count).map(|i| FeatureMutation::Create { feature: FeatureInput {
                feature_id: Some(MapFeatureId::new()), geometry: if batch == 0 && i == 0 { FeatureGeometry::LineString((0..20_000).map(|p| GeoJsonPosition::new(f64::from(p) / 20_000., 0., None)).collect()) } else { FeatureGeometry::Point(GeoJsonPosition::new(0., 0., None)) },
                properties: Default::default(), semantic_type: "inspection".into(), time: None,
                title: Some(format!("Inspection {batch}-{i}")), related_resources: vec![], evidence_resources: vec![],
            }}).collect();
            authoring.commit_changes(&identity, &scope, CommitFeatureChangesRequest {
                layer_id: layer.layer_id.clone(), expected_layer_revision: batch as u64,
                idempotency_key: format!("batch-{batch}"), mutations,
            }).await.unwrap();
        }
        let page = enumerate(&catalog, &analytics, &identity, &scope, &MapKnowledgePageUri::new(MapKnowledgeCollection::Features)).await.unwrap();
        assert_eq!(page.items.len(), 100);
        let last = page.items.last().unwrap().uri.clone();
        let second = enumerate(&catalog, &analytics, &identity, &scope, &MapKnowledgePageUri::new(MapKnowledgeCollection::Features).with_cursor(page.next_cursor.unwrap()).unwrap()).await.unwrap();
        assert_eq!(second.items.len(), 5);
        assert!(second.items.iter().all(|item| item.uri > last));
        assert!(second.next_cursor.is_none());
        let member = MapKnowledgeMember::parse(page.items[0].uri.as_str()).unwrap();
        let feature = read(&catalog, &analytics, &identity, &scope, &member).await.unwrap().unwrap();
        let (summary, observation) = feature.document().unwrap();
        assert!(summary.len() < 4096);
        let MapKnowledgeMember::Feature { layer: layer_id, feature: feature_id } = &member else { panic!("feature page member"); };
        let source = authoring.feature(&identity, &scope, layer_id, feature_id).await.unwrap().unwrap();
        let full = serde_json::to_string(&source).unwrap();
        assert!(full.len() > 64 * 1024, "full geometry stays available outside the bounded knowledge resource");
        let summary: MapKnowledgeSummary = serde_json::from_str(&summary).unwrap();
        assert_eq!(summary.source_sha256, content_digest(&full));
        assert_eq!(summary.source, member.source_uri());
        assert_eq!(observation.modified_by(), Some(&ModifiedBy::Principal(identity.actor.id.clone())));
        assert_ne!(read(&catalog, &analytics, &identity, &scope, &layer_address).await.unwrap().unwrap().document().unwrap().1.revision(), first_revision.revision());
        let publication = authoring.publish_layer(&identity, &scope, PublishFeatureLayerRequest { layer_id: layer.layer_id.clone(), expected_layer_revision: 2, title: Some("Published inspections".into()) }).await.unwrap();
        let publication_address = MapKnowledgeMember::Publication { layer: layer.layer_id.clone(), publication: publication.publication_id };
        let published = read(&catalog, &analytics, &identity, &scope, &publication_address).await.unwrap().unwrap().document().unwrap().1;
        assert_eq!(published.modified_by(), observation.modified_by());
        let mut wrong_context = identity.clone();
        wrong_context.authority.work_context = "other".parse().unwrap();
        assert!(read(&catalog, &analytics, &wrong_context, &scope, &member).await.unwrap().is_none());
        // A malformed denied parent and its 105 children must never reach decoding.
        db.a.client().query(include_str!("../queries/knowledge/tests/authoring_knowledge_uses_current_parent_access_before_decoding_and_pages_features/statement_1.surql")).await.unwrap().check().unwrap();
        for collection in [MapKnowledgeCollection::Layers, MapKnowledgeCollection::Features, MapKnowledgeCollection::Publications] {
            assert!(enumerate(&catalog, &analytics, &identity, &scope, &MapKnowledgePageUri::new(collection)).await.unwrap().items.is_empty());
        }
        assert!(read(&catalog, &analytics, &identity, &scope, &publication_address).await.unwrap().is_none());
        let mut cleared = identity;
        cleared.actor.data_labels.insert("secret".parse().unwrap());
        assert!(read(&catalog, &analytics, &cleared, &scope, &member).await.is_err());
    }).await.expect("Map knowledge qualification exceeded 120 seconds");
}

#[test]
fn escaped_titles_fit_page_and_search_budgets() {
    use veoveo_mcp_knowledge_extension::{SearchHit, SearchResults};
    let title = "\0\"\\".repeat(128);
    let now = Utc::now();
    let mut members = Vec::new();
    let mut hits = Vec::new();
    for _ in 0..100 {
        let location = MapLocation {
            location_id: LocationId::new(),
            name: title.clone(),
            position: Wgs84Position::new(0., 0., None).unwrap(),
            alternate_names: Default::default(),
            lineage: SourceLineage {
                release_id: DatasetReleaseId::new(),
                source_feature_id: "fixture".into(),
                authority: AuthorityClass::SyntheticTest,
                valid_from: now,
                valid_until: None,
            },
        };
        let member = geography::location(location).unwrap();
        hits.push(
            SearchHit::new(
                member.address.to_uri(),
                Some(summaries::link_title(&title)),
                None,
                None,
            )
            .unwrap(),
        );
        members.push(member);
    }
    assert!(serde_json::to_vec(&page(members)).unwrap().len() <= 64 * 1024);
    let results = SearchResults::new(hits).unwrap();
    assert!(serde_json::to_vec(&results).unwrap().len() <= 64 * 1024);
    #[cfg(feature = "mcp")]
    assert!(
        serde_json::to_vec(&veoveo_mcp_knowledge_extension::server::search_result(
            results
        ))
        .unwrap()
        .len()
            <= 128 * 1024
    );
}

#[test]
fn geographic_pages_and_search_share_exact_member_release_selection() {
    let root = tempfile::TempDir::new().unwrap();
    let analytics = analytics(&root);
    let releases = [DatasetReleaseId::new(), DatasetReleaseId::new()];
    let facility_id = FacilityId::new();
    let mut ids = (0..105).map(|_| LocationId::new()).collect::<Vec<_>>();
    ids.sort();
    for (n, release) in releases.iter().enumerate() {
        analytics
            .replace_release_products("tenant", release, |writer| {
                for id in &ids {
                    writer.put_location(
                        "tenant",
                        &MapLocation {
                            location_id: id.clone(),
                            name: format!("Warehouse {n}"),
                            position: Wgs84Position::new(179., 0., None).unwrap(),
                            alternate_names: Default::default(),
                            lineage: SourceLineage {
                                release_id: release.clone(),
                                source_feature_id: "source".into(),
                                authority: AuthorityClass::SyntheticTest,
                                valid_from: Utc::now(),
                                valid_until: None,
                            },
                        },
                    )?;
                }
                writer.put_facility(
                    "tenant",
                    &Facility {
                        facility_id: facility_id.clone(),
                        name: format!("Warehouse facility {n}"),
                        kind: FacilityKind::Warehouse,
                        position: Wgs84Position::new(179., 0., None).unwrap(),
                        supported_mobility_families: Default::default(),
                        transfer_map_families: Default::default(),
                        operating_intervals: vec![],
                        capabilities: Default::default(),
                        lineage: SourceLineage {
                            release_id: release.clone(),
                            source_feature_id: "facility".into(),
                            authority: AuthorityClass::SyntheticTest,
                            valid_from: Utc::now(),
                            valid_until: None,
                        },
                    },
                )?;
                Ok(())
            })
            .unwrap();
        analytics
            .activate_release("tenant", &MapDatasetId::new(), release)
            .unwrap();
    }
    let first = page(
        analytics
            .knowledge_page(
                "tenant",
                &MapKnowledgePageUri::new(MapKnowledgeCollection::Locations),
            )
            .unwrap(),
    );
    assert_eq!(first.items.len(), 100);
    let second = page(
        analytics
            .knowledge_page(
                "tenant",
                &MapKnowledgePageUri::new(MapKnowledgeCollection::Locations)
                    .with_cursor(first.next_cursor.unwrap())
                    .unwrap(),
            )
            .unwrap(),
    );
    assert_eq!(second.items.len(), 5);
    let query = SearchLocationsRequest {
        query: "Warehouse".into(),
        coverage: Wgs84BoundingBox {
            west: 170.,
            south: -5.,
            east: -170.,
            north: 5.,
        },
        include_facilities: true,
        limit: 100,
    };
    let hits = analytics.search_locations("tenant", &query).unwrap();
    assert_eq!(hits.results().len(), 100);
    let facility_page = analytics
        .knowledge_page(
            "tenant",
            &MapKnowledgePageUri::new(MapKnowledgeCollection::Facilities),
        )
        .unwrap();
    assert_eq!(facility_page.len(), 1);
    let mut facilities = query.clone();
    facilities.query = "facility".into();
    let facility_hits = analytics.search_locations("tenant", &facilities).unwrap();
    assert_eq!(facility_hits.results().len(), 1);
    assert_eq!(
        facility_hits.results()[0].uri(),
        &MapKnowledgeMember::Facility {
            facility: facility_id
        }
        .to_uri()
    );
    facilities.include_facilities = false;
    assert!(
        analytics
            .search_locations("tenant", &facilities)
            .unwrap()
            .results()
            .is_empty()
    );
    assert!(
        hits.results()
            .iter()
            .all(|hit| hit.title() == Some("Warehouse 0"))
    );
    assert!(
        analytics
            .search_locations("foreign", &query)
            .unwrap()
            .results()
            .is_empty()
    );
    assert!(
        analytics
            .knowledge_page(
                "foreign",
                &MapKnowledgePageUri::new(MapKnowledgeCollection::Locations)
            )
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        analytics.location("tenant", &ids[0]).unwrap().unwrap().name,
        "Warehouse 0"
    );
}
