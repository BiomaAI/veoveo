//! SQL admission for Map metadata; Docker fixture owns all records and cleanup.
use std::time::Duration;
use veoveo_map_mcp::persistence::MapRepository;
use veoveo_map_mcp::persistence::{
    MapAuthoringCompletion, MapAuthoringReadScope, MapCompositionDraft, MapCompositionRecord,
    MapCompositionRevisionDraft, MapFeatureCommitDraft, MapFeatureLayerDraft,
    MapFeatureLayerRecord, MapFeatureRevisionDraft, MapFeatureSchemaDraft, MapLayerProductDraft,
    MapLayerProductRecord, MapLayerPublicationDraft, MapLayerPublicationRecord,
    MapStyleRevisionDraft,
};

use chrono::Utc;
use uuid::Uuid;
use veoveo_platform_store::*;

#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "support/work_context.rs"]
mod work_context;

fn authority(context: &str, labels: &[&str]) -> InvocationAuthorityRecord {
    InvocationAuthorityRecord {
        context_key: context.into(),
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: "r1".into(),
        owner_kind: ArtifactGrantSubjectKind::Principal,
        owner_key: "author".into(),
        initial_grants: vec![],
        classification: None,
        data_labels: labels.iter().map(|s| (*s).to_owned()).collect(),
        invocation_mode: InvocationMode::Direct,
        initiator_key: None,
        delegation_id: None,
    }
}
struct Records {
    layer: MapFeatureLayerRecord,
    composition: MapCompositionRecord,
    publication: MapLayerPublicationRecord,
    product: MapLayerProductRecord,
}
async fn create_records(
    store: &PlatformStore,
    identity: &PlatformIdentity,
    context: &str,
    labels: &[&str],
) -> Records {
    let authority = authority(context, labels);
    let layer = MapRepository::new(store.clone())
        .create_map_feature_layer(MapFeatureLayerDraft {
            identity: identity.clone(),
            authority: authority.clone(),
            layer_key: veoveo_map_mcp::contract::FeatureLayerId::parse(format!(
                "feature-layer-{}",
                Uuid::now_v7()
            ))
            .unwrap(),
            title: "Fixture".into(),
            description: None,
            content_class: "boundaries".into(),
            schema: MapFeatureSchemaDraft {
                schema_revision_key: veoveo_map_mcp::contract::FeatureSchemaRevisionId::parse(
                    format!("feature-schema-{}", Uuid::now_v7()),
                )
                .unwrap(),
                schema_version: 1,
                digest_sha256: "a".repeat(64),
                schema_json: "{}".into(),
            },
            style: Some(MapStyleRevisionDraft {
                style_revision_key: veoveo_map_mcp::contract::StyleRevisionId::parse(format!(
                    "style-{}",
                    Uuid::now_v7()
                ))
                .unwrap(),
                style_version: 1,
                style_json: "{}".into(),
            }),
            revision: 0,
            archived_at: None,
            canonical_json: "{}".into(),
        })
        .await
        .unwrap();
    let publication = MapRepository::new(store.clone())
        .create_map_layer_publication(MapLayerPublicationDraft {
            identity: identity.clone(),
            authority: authority.clone(),
            publication_key: veoveo_map_mcp::contract::LayerPublicationId::parse(format!(
                "publication-{}",
                Uuid::now_v7()
            ))
            .unwrap(),
            layer_key: veoveo_map_mcp::contract::FeatureLayerId::parse(layer.layer_key.clone())
                .unwrap(),
            layer_revision: 0,
            schema_version: 1,
            style_revision_key: None,
            artifact_uris: vec![],
            canonical_json: "{}".into(),
            published_at: Utc::now(),
        })
        .await
        .unwrap();
    let product = MapRepository::new(store.clone())
        .create_map_layer_product(MapLayerProductDraft {
            identity: identity.clone(),
            authority: authority.clone(),
            product_key: veoveo_map_mcp::contract::LayerProductId::parse(format!(
                "layer-product-{}",
                Uuid::now_v7()
            ))
            .unwrap(),
            publication_key: veoveo_map_mcp::contract::LayerPublicationId::parse(
                publication.publication_key.clone(),
            )
            .unwrap(),
            layer_key: veoveo_map_mcp::contract::FeatureLayerId::parse(layer.layer_key.clone())
                .unwrap(),
            layer_revision: 0,
            format: veoveo_map_mcp::contract::LayerProductFormat::GeoJsonSeq,
            artifact_uri: veoveo_artifact_contract::ArtifactId::new().plane_uri(),
            mime_type: "application/geo+json-seq".into(),
            digest_sha256: "b".repeat(64),
            size_bytes: 128,
            feature_count: 1,
            canonical_json: "{}".into(),
            created_by_key: veoveo_types::PrincipalId::parse(identity.principal_key.clone())
                .unwrap(),
            created_at: Utc::now(),
        })
        .await
        .unwrap();
    let composition = MapRepository::new(store.clone())
        .create_map_composition(MapCompositionDraft {
            identity: identity.clone(),
            authority,
            composition_key: veoveo_map_mcp::contract::MapCompositionId::parse(format!(
                "composition-{}",
                Uuid::now_v7()
            ))
            .unwrap(),
            title: "Fixture".into(),
            revision: MapCompositionRevisionDraft {
                composition_revision_key:
                    veoveo_map_mcp::contract::MapCompositionRevisionId::parse(format!(
                        "composition-revision-{}",
                        Uuid::now_v7()
                    ))
                    .unwrap(),
                revision: 1,
                publication_keys: vec![
                    veoveo_map_mcp::contract::LayerPublicationId::parse(
                        &publication.publication_key,
                    )
                    .unwrap(),
                ],
                canonical_json: "{}".into(),
            },
            canonical_json: "{}".into(),
        })
        .await
        .unwrap();
    Records {
        layer,
        composition,
        publication,
        product,
    }
}

#[tokio::test]
async fn map_authoring_reads_enforce_labels_context_and_parent_visibility_in_sql() {
    tokio::time::timeout(Duration::from_secs(120), qualify())
        .await
        .expect("Map read qualification exceeded 120 seconds");
}
async fn qualify() {
    let db = fixture::TestDb::with_modules(vec![
        veoveo_map_mcp::schema::module_setup(fixture::module_lanes::execution("map").unwrap())
            .unwrap(),
    ])
    .await;
    let identity =
        db.a.ensure_identity(
            "map-read",
            "author",
            "https://fixture.local/services",
            "author",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    let foreign =
        db.a.ensure_identity(
            "foreign",
            "author",
            "https://fixture.local/services",
            "author",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    let visible = create_records(&db.a, &identity, "operations", &[]).await;
    let denied = create_records(&db.a, &identity, "operations", &["restricted", "secret"]).await;
    let private = create_records(&db.a, &identity, "private", &[]).await;
    let foreign = create_records(&db.a, &foreign, "operations", &[]).await;
    let scope =
        MapAuthoringReadScope::new("map-read", "operations", vec!["restricted".into()]).unwrap();
    assert_eq!(
        MapRepository::new(db.b.clone())
            .map_feature_layers_page(&scope, true, None, 100)
            .await
            .unwrap(),
        vec![visible.layer.clone()]
    );
    assert_eq!(
        MapRepository::new(db.b.clone())
            .map_compositions_page(&scope, true, None, 100)
            .await
            .unwrap(),
        vec![visible.composition.clone()]
    );
    assert_eq!(
        MapRepository::new(db.b.clone())
            .map_layer_publications_page(&scope, None, None, 100)
            .await
            .unwrap(),
        vec![visible.publication.clone()]
    );
    assert_eq!(
        MapRepository::new(db.b.clone())
            .map_layer_products_page(&scope, None, None, 100)
            .await
            .unwrap(),
        vec![visible.product.clone()]
    );
    for hidden in [&denied, &private, &foreign] {
        assert!(
            MapRepository::new(db.b.clone())
                .map_feature_layer(
                    &scope,
                    &veoveo_map_mcp::contract::FeatureLayerId::parse(&hidden.layer.layer_key)
                        .unwrap()
                )
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            MapRepository::new(db.b.clone())
                .map_composition(
                    &scope,
                    &veoveo_map_mcp::contract::MapCompositionId::parse(
                        &hidden.composition.composition_key
                    )
                    .unwrap()
                )
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            MapRepository::new(db.b.clone())
                .map_layer_product(
                    &scope,
                    &veoveo_map_mcp::contract::FeatureLayerId::parse(&hidden.layer.layer_key)
                        .unwrap(),
                    &veoveo_map_mcp::contract::LayerPublicationId::parse(
                        &hidden.publication.publication_key
                    )
                    .unwrap(),
                    &veoveo_map_mcp::contract::LayerProductId::parse(&hidden.product.product_key)
                        .unwrap()
                )
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            MapRepository::new(db.b.clone())
                .map_layer_publications_page(
                    &scope,
                    Some(&hidden.layer.layer_key)
                        .map(|id| veoveo_map_mcp::contract::FeatureLayerId::parse(id).unwrap())
                        .as_ref(),
                    None,
                    100
                )
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            MapRepository::new(db.b.clone())
                .map_layer_products_page(
                    &scope,
                    Some(&hidden.publication.publication_key)
                        .map(|id| veoveo_map_mcp::contract::LayerPublicationId::parse(id).unwrap())
                        .as_ref(),
                    None,
                    100
                )
                .await
                .unwrap()
                .is_empty()
        );
    }
    // A nested product URI must name both of its actual parents.
    for (layer, publication) in [
        (
            &visible.layer.layer_key,
            &denied.publication.publication_key,
        ),
        (
            &denied.layer.layer_key,
            &visible.publication.publication_key,
        ),
    ] {
        assert!(
            MapRepository::new(db.b.clone())
                .map_layer_product(
                    &scope,
                    &veoveo_map_mcp::contract::FeatureLayerId::parse(layer).unwrap(),
                    &veoveo_map_mcp::contract::LayerPublicationId::parse(publication).unwrap(),
                    &veoveo_map_mcp::contract::LayerProductId::parse(&visible.product.product_key)
                        .unwrap()
                )
                .await
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(
        MapRepository::new(db.b.clone())
            .map_layer_product(
                &scope,
                &veoveo_map_mcp::contract::FeatureLayerId::parse(&visible.layer.layer_key).unwrap(),
                &veoveo_map_mcp::contract::LayerPublicationId::parse(
                    &visible.publication.publication_key
                )
                .unwrap(),
                &veoveo_map_mcp::contract::LayerProductId::parse(&visible.product.product_key)
                    .unwrap()
            )
            .await
            .unwrap(),
        Some(visible.product.clone())
    );
    let clearance = MapAuthoringReadScope::new(
        "map-read",
        "operations",
        vec!["restricted".into(), "secret".into()],
    )
    .unwrap();
    assert_eq!(
        MapRepository::new(db.b.clone())
            .map_feature_layers_page(&clearance, true, None, 100)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        MapRepository::new(db.b.clone())
            .map_layer_product(
                &clearance,
                &veoveo_map_mcp::contract::FeatureLayerId::parse(&denied.layer.layer_key).unwrap(),
                &veoveo_map_mcp::contract::LayerPublicationId::parse(
                    &denied.publication.publication_key
                )
                .unwrap(),
                &veoveo_map_mcp::contract::LayerProductId::parse(&denied.product.product_key)
                    .unwrap()
            )
            .await
            .unwrap(),
        Some(denied.product.clone())
    );
    assert_eq!(
        MapRepository::new(db.b.clone())
            .map_composition(
                &clearance,
                &veoveo_map_mcp::contract::MapCompositionId::parse(
                    &denied.composition.composition_key
                )
                .unwrap()
            )
            .await
            .unwrap(),
        Some(denied.composition.clone())
    );
    assert_eq!(
        MapRepository::new(db.b.clone())
            .map_layer_publications_page(
                &clearance,
                Some(&denied.layer.layer_key)
                    .map(|id| veoveo_map_mcp::contract::FeatureLayerId::parse(id).unwrap())
                    .as_ref(),
                None,
                100
            )
            .await
            .unwrap(),
        vec![denied.publication]
    );
    assert_eq!(
        MapRepository::new(db.b.clone())
            .map_layer_products_page(
                &clearance,
                Some(&denied.product.publication_key)
                    .map(|id| veoveo_map_mcp::contract::LayerPublicationId::parse(id).unwrap())
                    .as_ref(),
                None,
                100
            )
            .await
            .unwrap(),
        vec![denied.product]
    );
    for (domain, expected) in [
        (
            MapAuthoringCompletion::Layer,
            visible.layer.layer_key.clone(),
        ),
        (
            MapAuthoringCompletion::SchemaVersion {
                layer: Some(visible.layer.layer_key.clone()),
            },
            "1".into(),
        ),
        (
            MapAuthoringCompletion::StyleVersion {
                layer: Some(visible.layer.layer_key.clone()),
            },
            "1".into(),
        ),
        (
            MapAuthoringCompletion::StyleRevision,
            visible.layer.style_revision_key.clone().unwrap(),
        ),
        (
            MapAuthoringCompletion::Publication {
                layer: Some(visible.layer.layer_key.clone()),
            },
            visible.publication.publication_key.clone(),
        ),
        (
            MapAuthoringCompletion::Product {
                layer: Some(visible.layer.layer_key.clone()),
                publication: Some(visible.publication.publication_key.clone()),
            },
            visible.product.product_key.clone(),
        ),
        (
            MapAuthoringCompletion::Composition,
            visible.composition.composition_key.clone(),
        ),
        (
            MapAuthoringCompletion::CompositionRevision {
                composition: Some(visible.composition.composition_key.clone()),
            },
            "1".into(),
        ),
    ] {
        assert_eq!(
            MapRepository::new(db.b.clone())
                .complete_map_authoring(&scope, domain.clone(), "")
                .await
                .unwrap(),
            vec![expected.clone()]
        );
        assert_eq!(
            MapRepository::new(db.b.clone())
                .complete_map_authoring(&scope, domain.clone(), &expected.to_uppercase())
                .await
                .unwrap(),
            vec![expected]
        );
        assert!(
            MapRepository::new(db.b.clone())
                .complete_map_authoring(&scope, domain, "' OR true --")
                .await
                .unwrap()
                .is_empty()
        );
    }
    assert!(
        MapRepository::new(db.b.clone())
            .complete_map_authoring(
                &scope,
                MapAuthoringCompletion::Publication {
                    layer: Some(denied.layer.layer_key.clone())
                },
                ""
            )
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        MapRepository::new(db.b.clone())
            .complete_map_authoring(
                &scope,
                MapAuthoringCompletion::Product {
                    layer: Some(visible.layer.layer_key.clone()),
                    publication: Some(private.publication.publication_key.clone())
                },
                ""
            )
            .await
            .unwrap()
            .is_empty()
    );
    // Deduplicate versions in SQL even when multiple visible layers share them.
    assert_eq!(
        MapRepository::new(db.b.clone())
            .complete_map_authoring(
                &clearance,
                MapAuthoringCompletion::SchemaVersion { layer: None },
                ""
            )
            .await
            .unwrap(),
        vec!["1"]
    );
    // Removed parent records revoke both collection and exact product reads.
    db.a.client()
        .query(include_str!(
            "queries/map_authoring_reads/qualify/statement_1.surql"
        ))
        .bind(("layer", visible.layer.id))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        MapRepository::new(db.b.clone())
            .map_layer_publications_page(&scope, None, None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        MapRepository::new(db.b.clone())
            .map_layer_products_page(&scope, None, None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        MapRepository::new(db.b.clone())
            .map_layer_product(
                &scope,
                &veoveo_map_mcp::contract::FeatureLayerId::parse(&visible.layer.layer_key).unwrap(),
                &veoveo_map_mcp::contract::LayerPublicationId::parse(
                    &visible.publication.publication_key
                )
                .unwrap(),
                &veoveo_map_mcp::contract::LayerProductId::parse(&visible.product.product_key)
                    .unwrap()
            )
            .await
            .unwrap()
            .is_none()
    );
    // Archive filtering composes with visibility inside the same query.
    db.a.client()
        .query(include_str!(
            "queries/map_authoring_reads/qualify/statement_2.surql"
        ))
        .bind(("record", denied.layer.id))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        MapRepository::new(db.b.clone())
            .map_feature_layers_page(&clearance, false, None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        MapRepository::new(db.b.clone())
            .map_feature_layers_page(&clearance, true, None, 100)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[derive(Clone, Copy)]
enum Index {
    Layers,
    Publications,
    Products,
    Compositions,
}

async fn page_keys(
    store: &PlatformStore,
    scope: &MapAuthoringReadScope,
    index: Index,
    after: Option<&str>,
) -> Vec<String> {
    match index {
        Index::Layers => MapRepository::new(store.clone())
            .map_feature_layers_page(
                scope,
                false,
                after
                    .map(|id| veoveo_map_mcp::contract::FeatureLayerId::parse(id).unwrap())
                    .as_ref(),
                100,
            )
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.layer_key)
            .collect(),
        Index::Publications => MapRepository::new(store.clone())
            .map_layer_publications_page(
                scope,
                None,
                after
                    .map(|id| veoveo_map_mcp::contract::LayerPublicationId::parse(id).unwrap())
                    .as_ref(),
                100,
            )
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.publication_key)
            .collect(),
        Index::Products => MapRepository::new(store.clone())
            .map_layer_products_page(
                scope,
                None,
                after
                    .map(|id| veoveo_map_mcp::contract::LayerProductId::parse(id).unwrap())
                    .as_ref(),
                100,
            )
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.product_key)
            .collect(),
        Index::Compositions => MapRepository::new(store.clone())
            .map_compositions_page(
                scope,
                false,
                after
                    .map(|id| veoveo_map_mcp::contract::MapCompositionId::parse(id).unwrap())
                    .as_ref(),
                100,
            )
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.composition_key)
            .collect(),
    }
}

#[tokio::test]
async fn metadata_pages_apply_current_visibility_and_parents_before_limits() {
    tokio::time::timeout(Duration::from_secs(120), qualify_pages())
        .await
        .expect("metadata page qualification exceeded 120 seconds");
}

async fn qualify_pages() {
    let db = fixture::TestDb::with_modules(vec![
        veoveo_map_mcp::schema::module_setup(fixture::module_lanes::execution("map").unwrap())
            .unwrap(),
    ])
    .await;
    let identity =
        db.a.ensure_identity(
            "map-pages",
            "author",
            "https://fixture.local",
            "author",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    let foreign =
        db.a.ensure_identity(
            "foreign-pages",
            "author",
            "https://fixture.local",
            "author",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    // All three denied sets are created before admitted rows, so a limit placed
    // before tenant, context, or label admission would produce incomplete pages.
    for _ in 0..110 {
        create_records(&db.a, &foreign, "operations", &["restricted"]).await;
    }
    for _ in 0..110 {
        create_records(&db.a, &identity, "private", &["restricted"]).await;
    }
    for _ in 0..110 {
        create_records(&db.a, &identity, "operations", &["restricted", "secret"]).await;
    }
    let archived = create_records(&db.a, &identity, "operations", &["restricted"]).await;
    db.a.client()
        .query(include_str!(
            "queries/map_authoring_reads/qualify_pages/statement_1.surql"
        ))
        .bind(("layer", archived.layer.id.clone()))
        .bind(("composition", archived.composition.id.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let mut rows = Vec::new();
    for _ in 0..125 {
        rows.push(create_records(&db.a, &identity, "operations", &["restricted"]).await);
    }
    let scope =
        MapAuthoringReadScope::new("map-pages", "operations", vec!["restricted".into()]).unwrap();
    let revoked = MapAuthoringReadScope::new("map-pages", "operations", vec![]).unwrap();
    for index in [
        Index::Layers,
        Index::Publications,
        Index::Products,
        Index::Compositions,
    ] {
        let key = |row: &Records| match index {
            Index::Layers => row.layer.layer_key.clone(),
            Index::Publications => row.publication.publication_key.clone(),
            Index::Products => row.product.product_key.clone(),
            Index::Compositions => row.composition.composition_key.clone(),
        };
        let mut expected: Vec<_> = rows.iter().map(key).collect();
        // An archived layer's immutable publications/products remain readable.
        if matches!(index, Index::Publications | Index::Products) {
            expected.push(key(&archived));
        }
        expected.sort();
        let first = page_keys(&db.b, &scope, index, None).await;
        assert_eq!(first, expected[..100]);
        let after = first.last().unwrap();
        let second = page_keys(&db.b, &scope, index, Some(after)).await;
        assert_eq!(second, expected[100..]);
        assert!(
            page_keys(&db.b, &scope, index, second.last().map(String::as_str))
                .await
                .is_empty()
        );
        // Continuations reapply clearance; a previously issued key grants no access.
        assert!(
            page_keys(&db.b, &revoked, index, Some(after))
                .await
                .is_empty()
        );
    }
    let last = rows.last().unwrap();
    assert_eq!(
        MapRepository::new(db.b.clone())
            .map_layer_publications_page(
                &scope,
                Some(&last.layer.layer_key)
                    .map(|id| veoveo_map_mcp::contract::FeatureLayerId::parse(id).unwrap())
                    .as_ref(),
                None,
                101
            )
            .await
            .unwrap(),
        vec![last.publication.clone()]
    );
    assert_eq!(
        MapRepository::new(db.b.clone())
            .map_layer_products_page(
                &scope,
                Some(&last.publication.publication_key)
                    .map(|id| veoveo_map_mcp::contract::LayerPublicationId::parse(id).unwrap())
                    .as_ref(),
                None,
                101
            )
            .await
            .unwrap(),
        vec![last.product.clone()]
    );
    assert!(
        MapRepository::new(db.b.clone())
            .map_layer_publications_page(
                &revoked,
                Some(&last.layer.layer_key)
                    .map(|id| veoveo_map_mcp::contract::FeatureLayerId::parse(id).unwrap())
                    .as_ref(),
                None,
                101
            )
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        MapRepository::new(db.b.clone())
            .map_layer_products_page(
                &revoked,
                Some(&last.publication.publication_key)
                    .map(|id| veoveo_map_mcp::contract::LayerPublicationId::parse(id).unwrap())
                    .as_ref(),
                None,
                101
            )
            .await
            .unwrap()
            .is_empty()
    );
    for limit in [0, 102] {
        assert!(
            MapRepository::new(db.b.clone())
                .map_feature_layers_page(&scope, false, None, limit)
                .await
                .is_err()
        );
        assert!(
            MapRepository::new(db.b.clone())
                .map_compositions_page(&scope, false, None, limit)
                .await
                .is_err()
        );
        assert!(
            MapRepository::new(db.b.clone())
                .map_layer_publications_page(&scope, None, None, limit)
                .await
                .is_err()
        );
        assert!(
            MapRepository::new(db.b.clone())
                .map_layer_products_page(&scope, None, None, limit)
                .await
                .is_err()
        );
    }
    assert!(veoveo_map_mcp::contract::FeatureLayerId::parse("invalid").is_err());
    // Parent deletion also changes a resumed publication/product selection.
    db.a.client()
        .query(include_str!(
            "queries/map_authoring_reads/qualify_pages/statement_2.surql"
        ))
        .bind(("layer", last.layer.id.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        MapRepository::new(db.b.clone())
            .map_layer_publications_page(
                &scope,
                Some(&last.layer.layer_key)
                    .map(|id| veoveo_map_mcp::contract::FeatureLayerId::parse(id).unwrap())
                    .as_ref(),
                None,
                101
            )
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        MapRepository::new(db.b.clone())
            .map_layer_products_page(
                &scope,
                Some(&last.publication.publication_key)
                    .map(|id| veoveo_map_mcp::contract::LayerPublicationId::parse(id).unwrap())
                    .as_ref(),
                None,
                101
            )
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn native_map_commit_order_is_transactional_and_independent_of_other_domains() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::with_modules(vec![
            veoveo_map_mcp::schema::module_setup(fixture::module_lanes::execution("map").unwrap())
                .unwrap(),
        ])
        .await;
        let identity =
            db.a.ensure_identity(
                "map-order",
                "author",
                "https://identity.test",
                "author",
                PrincipalKind::User,
            )
            .await
            .unwrap();
        work_context::install(&db.a, &identity, &authority("operations", &[])).await;
        let first = create_records(&db.a, &identity, "operations", &[]).await;
        let second = create_records(&db.b, &identity, "operations", &[]).await;
        let draft = |layer: &MapFeatureLayerRecord| MapFeatureCommitDraft {
            identity: identity.clone(),
            authority: authority("operations", &[]),
            layer_key: veoveo_map_mcp::contract::FeatureLayerId::parse(layer.layer_key.clone())
                .unwrap(),
            layer_canonical_json: r#"{"revision":1}"#.into(),
            expected_layer_revision: 0,
            changeset_key: veoveo_map_mcp::contract::FeatureChangeSetId::parse(format!(
                "changeset-{}",
                Uuid::now_v7()
            ))
            .unwrap(),
            idempotency_key: Uuid::now_v7().to_string(),
            request_digest_sha256: "b".repeat(64),
            changeset_canonical_json: r#"{"resulting_layer_revision":1}"#.into(),
            revisions: vec![MapFeatureRevisionDraft {
                feature_key: veoveo_map_mcp::contract::MapFeatureId::parse(format!(
                    "feature-{}",
                    Uuid::now_v7()
                ))
                .unwrap(),
                feature_revision: 1,
                layer_revision: 1,
                schema_version: 1,
                deleted: false,
                geometry_type: "Point".into(),
                geometry_json: r#"{"type":"Point","coordinates":[-89.2,13.7]}"#.into(),
                bbox_west: -89.2,
                bbox_south: 13.7,
                bbox_east: -89.2,
                bbox_north: 13.7,
                valid_from: None,
                valid_until: None,
                semantic_type: "inspection_area".into(),
                title: None,
                canonical_json: r#"{"type":"Feature"}"#.into(),
                expected_feature_revision: None,
            }],
        };
        let one = draft(&first.layer);
        let two = draft(&second.layer);
        let first_writer = MapRepository::new(db.a.clone());
        let second_writer = MapRepository::new(db.b.clone());
        let (a, b) = tokio::join!(
            first_writer.commit_map_feature_changes(one.clone()),
            second_writer.commit_map_feature_changes(two.clone())
        );
        let mut sequences = Vec::new();
        for (store, result, draft) in [(&db.a, a, one.clone()), (&db.b, b, two)] {
            let committed = match result {
                Ok(committed) => committed,
                Err(veoveo_map_mcp::persistence::MapStoreError::Database(error))
                    if matches!(
                        error.query_details(),
                        Some(surrealdb::types::QueryError::TransactionConflict)
                    ) =>
                {
                    MapRepository::new(store.clone())
                        .commit_map_feature_changes(draft)
                        .await
                        .unwrap()
                }
                Err(error) => panic!("unexpected Map commit failure: {error}"),
            };
            sequences.push(committed.changeset.commit_sequence);
        }
        sequences.sort();
        assert_eq!(sequences, vec![1, 2]);
        assert_eq!(
            MapRepository::new(db.a.clone())
                .latest_map_feature_commit_sequence()
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            MapRepository::new(db.a.clone())
                .read_map_feature_commits(0, 2, 1)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            MapRepository::new(db.b.clone())
                .read_map_feature_commits(1, 2, 1)
                .await
                .unwrap()[0]
                .commit_sequence,
            2
        );
        assert!(
            MapRepository::new(db.a.clone())
                .commit_map_feature_changes(one.clone())
                .await
                .is_ok(),
            "idempotent replay"
        );
        let mut conflict = one;
        conflict.changeset_key = veoveo_map_mcp::contract::FeatureChangeSetId::new();
        conflict.idempotency_key = Uuid::now_v7().to_string();
        assert!(
            MapRepository::new(db.a.clone())
                .commit_map_feature_changes(conflict)
                .await
                .is_err()
        );
        assert_eq!(
            MapRepository::new(db.a.clone())
                .latest_map_feature_commit_sequence()
                .await
                .unwrap(),
            2,
            "rejected writes cannot advance the head"
        );
    })
    .await
    .expect("Map commit-order qualification deadline");
}
