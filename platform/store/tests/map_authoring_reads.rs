//! SQL admission for Map metadata; Docker fixture owns all records and cleanup.
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;
use veoveo_platform_store::*;

#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

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
    let layer = store
        .create_map_feature_layer(MapFeatureLayerDraft {
            identity: identity.clone(),
            authority: authority.clone(),
            layer_key: format!("feature-layer-{}", Uuid::now_v7()),
            title: "Fixture".into(),
            description: None,
            content_class: "boundaries".into(),
            schema: MapFeatureSchemaDraft {
                schema_revision_key: format!("feature-schema-{}", Uuid::now_v7()),
                schema_version: 1,
                digest_sha256: "a".repeat(64),
                schema_json: "{}".into(),
            },
            style: None,
            revision: 0,
            archived_at: None,
            canonical_json: "{}".into(),
        })
        .await
        .unwrap();
    let publication = store
        .create_map_layer_publication(MapLayerPublicationDraft {
            identity: identity.clone(),
            authority: authority.clone(),
            publication_key: format!("publication-{}", Uuid::now_v7()),
            layer_key: layer.layer_key.clone(),
            layer_revision: 0,
            schema_version: 1,
            style_revision_key: None,
            artifact_uris: vec![],
            canonical_json: "{}".into(),
            published_at: Utc::now(),
        })
        .await
        .unwrap();
    let product = store
        .create_map_layer_product(MapLayerProductDraft {
            identity: identity.clone(),
            authority: authority.clone(),
            product_key: format!("product-{}", Uuid::now_v7()),
            publication_key: publication.publication_key.clone(),
            layer_key: layer.layer_key.clone(),
            layer_revision: 0,
            format: "geojson_seq".into(),
            artifact_uri: format!("artifact://{}", Uuid::now_v7()),
            mime_type: "application/geo+json-seq".into(),
            digest_sha256: "b".repeat(64),
            size_bytes: 128,
            feature_count: 1,
            canonical_json: "{}".into(),
            created_by_key: identity.principal_key.clone(),
            created_at: Utc::now(),
        })
        .await
        .unwrap();
    let composition = store
        .create_map_composition(MapCompositionDraft {
            identity: identity.clone(),
            authority,
            composition_key: format!("composition-{}", Uuid::now_v7()),
            title: "Fixture".into(),
            revision: MapCompositionRevisionDraft {
                composition_revision_key: format!("composition-revision-{}", Uuid::now_v7()),
                revision: 1,
                publication_keys: vec![publication.publication_key.clone()],
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
    let db = fixture::TestDb::new().await;
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
        db.b.list_map_feature_layers(&scope, true).await.unwrap(),
        vec![visible.layer.clone()]
    );
    assert_eq!(
        db.b.list_map_compositions(&scope, true).await.unwrap(),
        vec![visible.composition.clone()]
    );
    assert_eq!(
        db.b.list_map_layer_publications(&scope, None)
            .await
            .unwrap(),
        vec![visible.publication.clone()]
    );
    assert_eq!(
        db.b.list_map_layer_products(&scope, None).await.unwrap(),
        vec![visible.product.clone()]
    );
    for hidden in [&denied, &private, &foreign] {
        assert!(
            db.b.map_feature_layer(&scope, &hidden.layer.layer_key)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            db.b.map_composition(&scope, &hidden.composition.composition_key)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            db.b.map_layer_product(&scope, &hidden.product.product_key)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            db.b.list_map_layer_publications(&scope, Some(&hidden.layer.layer_key))
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            db.b.list_map_layer_products(&scope, Some(&hidden.publication.publication_key))
                .await
                .unwrap()
                .is_empty()
        );
    }
    let clearance = MapAuthoringReadScope::new(
        "map-read",
        "operations",
        vec!["restricted".into(), "secret".into()],
    )
    .unwrap();
    assert_eq!(
        db.b.list_map_feature_layers(&clearance, true)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        db.b.map_layer_product(&clearance, &denied.product.product_key)
            .await
            .unwrap(),
        Some(denied.product.clone())
    );
    assert_eq!(
        db.b.map_composition(&clearance, &denied.composition.composition_key)
            .await
            .unwrap(),
        Some(denied.composition.clone())
    );
    assert_eq!(
        db.b.list_map_layer_publications(&clearance, Some(&denied.layer.layer_key))
            .await
            .unwrap(),
        vec![denied.publication]
    );
    assert_eq!(
        db.b.list_map_layer_products(&clearance, Some(&denied.product.publication_key))
            .await
            .unwrap(),
        vec![denied.product]
    );
    // Removed parent records revoke both collection and exact product reads.
    db.a.client()
        .query("DELETE ONLY $layer;")
        .bind(("layer", visible.layer.id))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        db.b.list_map_layer_publications(&scope, None)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        db.b.list_map_layer_products(&scope, None)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        db.b.map_layer_product(&scope, &visible.product.product_key)
            .await
            .unwrap()
            .is_none()
    );
    // Archive filtering composes with visibility inside the same query.
    db.a.client()
        .query("UPDATE ONLY $record SET archived_at = time::now();")
        .bind(("record", denied.layer.id))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        db.b.list_map_feature_layers(&clearance, false)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.b.list_map_feature_layers(&clearance, true)
            .await
            .unwrap()
            .len(),
        1
    );
}
