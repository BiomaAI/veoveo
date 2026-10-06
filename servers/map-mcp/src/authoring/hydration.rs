//! Selected rows must agree with retained documents before callers use them.
use super::service::decode;
use crate::{contract::*, persistence::*};
use anyhow::{Result, ensure};
use surrealdb::types::{Array, RecordId};

fn record(table: &str, tenant: &str, parts: &[&str]) -> RecordId {
    let mut key = vec![tenant.to_owned()];
    key.extend(parts.iter().map(|part| (*part).to_owned()));
    RecordId::new(table, Array::from(key))
}

pub(super) fn layer(row: MapFeatureLayerRecord, tenant_key: &str) -> Result<FeatureLayer> {
    let value: FeatureLayer = decode(&row.canonical_json, "feature layer")?;
    ensure!(
        row.id == record("map_feature_layer", tenant_key, &[&row.layer_key])
            && value.layer_id.as_str() == row.layer_key
            && i64::try_from(value.revision)? == row.revision
            && value.schema.schema_revision_id.as_str() == row.schema_revision_key
            && i64::try_from(value.schema.version)? == row.schema_version
            && value
                .style
                .as_ref()
                .map(|style| style.style_revision_id.to_string())
                == row.style_revision_key
            && value
                .style
                .as_ref()
                .map(|style| i64::try_from(style.version))
                .transpose()?
                == row.style_version
            && value.work_context.as_str() == row.authority.context_key
            && value.created_by.as_str() == row.created_by_key
            && value.archived_at == row.archived_at
            && super::service::subject_record(&value.owner)
                == (row.owner_kind, row.owner_key.clone())
            && value.classification.as_ref().map(ToString::to_string) == row.classification
            && value
                .data_labels
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                == row.data_labels,
        "feature layer document disagrees with selected metadata"
    );
    Ok(value)
}

pub(super) fn publication(
    row: MapLayerPublicationRecord,
    tenant_key: &str,
) -> Result<LayerPublication> {
    let value: LayerPublication = decode(&row.canonical_json, "layer publication")?;
    ensure!(
        row.id
            == record(
                "map_layer_publication",
                tenant_key,
                &[&row.layer_key, &row.publication_key]
            )
            && value.publication_id.as_str() == row.publication_key
            && value.layer_id.as_str() == row.layer_key
            && i64::try_from(value.layer_revision)? == row.layer_revision
            && i64::try_from(value.schema_version)? == row.schema_version
            && value.style_revision_id.as_ref().map(ToString::to_string) == row.style_revision_key
            && value
                .artifact_uris
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                == row.artifact_uris
            && value.work_context.as_str() == row.work_context_key
            && value.published_by.as_str() == row.published_by_key,
        "publication document disagrees with selected identity or metadata"
    );
    Ok(value)
}

pub(super) fn product(row: MapLayerProductRecord, tenant_key: &str) -> Result<LayerProduct> {
    let value: LayerProduct = decode(&row.canonical_json, "layer product")?;
    ensure!(
        row.id == record("map_layer_product", tenant_key, &[&row.product_key])
            && value.product_id.as_str() == row.product_key
            && value.publication_id.as_str() == row.publication_key
            && value.layer_id.as_str() == row.layer_key
            && i64::try_from(value.layer_revision)? == row.layer_revision
            && value.artifact_uri.as_str() == row.artifact_uri
            && crate::persistence::product_format(value.format) == row.format
            && value.mime_type == row.mime_type
            && value.digest_sha256 == row.digest_sha256
            && i64::try_from(value.size_bytes)? == row.size_bytes
            && i64::try_from(value.feature_count)? == row.feature_count
            && value.work_context.as_str() == row.authority.context_key
            && value.created_by.as_str() == row.created_by_key,
        "product document disagrees with selected identity or metadata"
    );
    Ok(value)
}

pub(super) fn composition(row: MapCompositionRecord, tenant_key: &str) -> Result<MapComposition> {
    let value: MapComposition = decode(&row.canonical_json, "map composition")?;
    ensure!(
        row.id == record("map_composition", tenant_key, &[&row.composition_key])
            && value.composition_id.as_str() == row.composition_key
            && i64::try_from(value.current.revision)? == row.current_revision
            && value.title == row.title
            && value.archived_at == row.archived_at
            && value.work_context.as_str() == row.authority.context_key
            && value.created_by.as_str() == row.created_by_key
            && super::service::subject_record(&value.owner)
                == (row.owner_kind, row.owner_key.clone())
            && super::service::subject_record(&value.owner)
                == (row.authority.owner_kind, row.authority.owner_key.clone())
            && value.classification.as_ref().map(ToString::to_string)
                == row.authority.classification
            && value
                .data_labels
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                == row.authority.data_labels,
        "composition document disagrees with selected identity or metadata"
    );
    Ok(value)
}

pub(super) fn composition_revision(
    row: MapCompositionRevisionRecord,
    tenant_key: &str,
) -> Result<MapCompositionRevision> {
    let value: MapCompositionRevision = decode(&row.canonical_json, "map composition revision")?;
    let revision = format!("{:020}", row.revision);
    ensure!(
        row.id
            == record(
                "map_composition_revision",
                tenant_key,
                &[&row.composition_key, &revision]
            )
            && value.composition_id.as_str() == row.composition_key
            && value.composition_revision_id.as_str() == row.composition_revision_key
            && i64::try_from(value.revision)? == row.revision
            && value
                .layers
                .iter()
                .map(|layer| layer.publication_id.to_string())
                .collect::<Vec<_>>()
                == row.publication_keys,
        "composition revision disagrees with selected identity or metadata"
    );
    Ok(value)
}

pub(super) fn feature_head(row: MapFeatureHeadRecord, tenant: &str) -> Result<MapFeature> {
    let value: MapFeature = decode(&row.canonical_json, "map feature")?;
    ensure!(
        row.id
            == record(
                "map_feature_head",
                tenant,
                &[&row.layer_key, &row.feature_key]
            )
            && value.id.as_str() == row.feature_key
            && value.layer_id.as_str() == row.layer_key
            && i64::try_from(value.feature_revision)? == row.feature_revision
            && i64::try_from(value.layer_revision)? == row.layer_revision
            && i64::try_from(value.schema_version)? == row.schema_version
            && value.deleted == row.deleted,
        "feature document disagrees with selected metadata"
    );
    Ok(value)
}

pub(super) fn feature_revision(row: &MapFeatureRevisionRecord, tenant: &str) -> Result<MapFeature> {
    let value: MapFeature = decode(&row.canonical_json, "map feature revision")?;
    let revision = format!("{:020}", row.feature_revision);
    ensure!(
        row.id
            == record(
                "map_feature_revision",
                tenant,
                &[&row.layer_key, &row.feature_key, &revision]
            )
            && value.id.as_str() == row.feature_key
            && value.layer_id.as_str() == row.layer_key
            && i64::try_from(value.feature_revision)? == row.feature_revision
            && i64::try_from(value.layer_revision)? == row.layer_revision
            && i64::try_from(value.schema_version)? == row.schema_version
            && value.deleted == row.deleted,
        "feature revision disagrees with selected metadata"
    );
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_platform_store::{
        ArtifactGrantSubjectKind, InvocationAuthorityRecord, WorkContextMembershipLevel,
    };
    #[test]
    fn publication_hydration_rejects_selected_parent_and_physical_identity_disagreement() {
        let now = chrono::Utc::now();
        let value = LayerPublication::new(LayerPublicationValue {
            publication_id: LayerPublicationId::new(),
            layer_id: FeatureLayerId::new(),
            layer_revision: 0,
            schema_version: 1,
            style_revision_id: None,
            title: Some("Published".into()),
            artifact_uris: vec![veoveo_artifact_contract::ArtifactId::new().plane_uri()],
            published_by: veoveo_types::PrincipalId::parse("author").unwrap(),
            work_context: veoveo_types::WorkContextId::parse("work").unwrap(),
            published_at: now,
        })
        .unwrap();
        let native = MapLayerPublicationRecord {
            id: record(
                "map_layer_publication",
                "tenant",
                &[value.layer_id.as_str(), value.publication_id.as_str()],
            ),
            tenant: RecordId::new("tenant", "fixture"),
            owner: RecordId::new("principal", "fixture"),
            work_context: RecordId::new("work_context", "fixture"),
            published_by_key: value.published_by.to_string(),
            work_context_key: value.work_context.to_string(),
            authority: InvocationAuthorityRecord {
                context_key: "work".into(),
                membership: WorkContextMembershipLevel::Owner,
                policy_revision: "r1".into(),
                owner_kind: ArtifactGrantSubjectKind::Principal,
                owner_key: "author".into(),
                initial_grants: vec![],
                classification: None,
                data_labels: vec![],
                invocation_mode: veoveo_platform_store::InvocationMode::Direct,
                initiator_key: None,
                delegation_id: None,
            },
            publication_key: value.publication_id.to_string(),
            layer_key: value.layer_id.to_string(),
            layer_revision: 0,
            schema_version: 1,
            style_revision_key: None,
            artifact_uris: value
                .artifact_uris
                .iter()
                .map(ToString::to_string)
                .collect(),
            canonical_json: serde_json::to_string(&value).unwrap(),
            published_at: now,
        };
        assert_eq!(publication(native.clone(), "tenant").unwrap(), value);
        let mut wrong = native.clone();
        wrong.layer_key = FeatureLayerId::new().to_string();
        assert!(publication(wrong, "tenant").is_err());
        let mut wrong = native.clone();
        wrong.id = record(
            "map_layer_publication",
            "foreign",
            &[&native.layer_key, &native.publication_key],
        );
        assert!(publication(wrong, "tenant").is_err());
        let mut wrong = native.clone();
        wrong.schema_version = 2;
        assert!(publication(wrong, "tenant").is_err());
        assert_eq!(
            native.canonical_json,
            serde_json::to_string(&value).unwrap()
        );
    }

    #[tokio::test]
    async fn selected_authoring_policy_contradictions_fail_after_sql_visibility() {
        tokio::time::timeout(std::time::Duration::from_secs(180), policy_contradictions())
            .await
            .expect("authoring hydration qualification exceeded 180 seconds");
    }

    async fn policy_contradictions() {
        use veoveo_types::{AccessSubject, DataLabelId, PrincipalId, WorkContextId};
        let db = crate::test_store::TestDb::with_modules(vec![
            crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap())
                .unwrap(),
        ])
        .await;
        let identity =
            db.a.ensure_identity(
                "hydration-policy",
                "author",
                "https://veoveo.local/services",
                "author",
                veoveo_platform_store::PrincipalKind::Service,
            )
            .await
            .unwrap();
        let authority = veoveo_platform_store::InvocationAuthorityRecord {
            context_key: "operations".into(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: "r1".into(),
            owner_kind: ArtifactGrantSubjectKind::Principal,
            owner_key: "author".into(),
            initial_grants: vec![],
            classification: Some("sensitive".into()),
            data_labels: vec!["sensitive".into()],
            invocation_mode: veoveo_platform_store::InvocationMode::Direct,
            initiator_key: Some("author".into()),
            delegation_id: None,
        };
        let now = chrono::Utc::now();
        let layer_id = FeatureLayerId::new();
        let layer_value = FeatureLayer::new(FeatureLayerValue {
            layer_id: layer_id.clone(),
            title: "Policy layer".into(),
            description: None,
            content_class: FeatureContentClass::Boundaries,
            schema: FeatureSchemaRevision {
                schema_revision_id: FeatureSchemaRevisionId::new(),
                layer_id: layer_id.clone(),
                version: 1,
                digest_sha256: "a".repeat(64),
                schema: serde_json::json!({"type":"object"}),
                created_at: now,
            },
            style: None,
            revision: 0,
            owner: AccessSubject::Principal(PrincipalId::parse("author").unwrap()),
            created_by: PrincipalId::parse("author").unwrap(),
            work_context: WorkContextId::parse("operations").unwrap(),
            classification: Some(DataLabelId::parse("sensitive").unwrap()),
            data_labels: [DataLabelId::parse("sensitive").unwrap()]
                .into_iter()
                .collect(),
            archived_at: None,
            created_at: now,
            updated_at: now,
        })
        .unwrap();
        let repository = MapRepository::new(db.a.clone());
        let layer_row = repository
            .create_map_feature_layer(MapFeatureLayerDraft {
                identity: identity.clone(),
                authority: authority.clone(),
                layer_key: layer_id.clone(),
                title: layer_value.title.clone(),
                description: None,
                content_class: "boundaries".into(),
                schema: MapFeatureSchemaDraft {
                    schema_revision_key: layer_value.schema.schema_revision_id.clone(),
                    schema_version: 1,
                    digest_sha256: layer_value.schema.digest_sha256.clone(),
                    schema_json: serde_json::to_string(&layer_value.schema.schema).unwrap(),
                },
                style: None,
                revision: 0,
                archived_at: None,
                canonical_json: serde_json::to_string(&layer_value).unwrap(),
            })
            .await
            .unwrap();
        let composition_id = MapCompositionId::new();
        let revision = MapCompositionRevision::new(MapCompositionRevisionValue {
            composition_revision_id: MapCompositionRevisionId::new(),
            composition_id: composition_id.clone(),
            revision: 1,
            layers: vec![],
            view: CompositionView {
                center: Wgs84Position::new(-89.2, 13.7, None).unwrap(),
                zoom: 8.0,
                bearing_deg: 0.0,
                pitch_deg: 0.0,
            },
            created_by: PrincipalId::parse("author").unwrap(),
            created_at: now,
        })
        .unwrap();
        let composition_value = MapComposition::new(MapCompositionValue {
            composition_id: composition_id.clone(),
            title: "Policy composition".into(),
            current: revision.clone(),
            owner: layer_value.owner.clone(),
            created_by: layer_value.created_by.clone(),
            work_context: layer_value.work_context.clone(),
            classification: layer_value.classification.clone(),
            data_labels: layer_value.data_labels.clone(),
            archived_at: None,
            created_at: now,
            updated_at: now,
        })
        .unwrap();
        let composition_row = repository
            .create_map_composition(MapCompositionDraft {
                identity: identity.clone(),
                authority,
                composition_key: composition_id.clone(),
                title: composition_value.title.clone(),
                revision: MapCompositionRevisionDraft {
                    composition_revision_key: revision.composition_revision_id.clone(),
                    revision: 1,
                    publication_keys: vec![],
                    canonical_json: serde_json::to_string(&revision).unwrap(),
                },
                canonical_json: serde_json::to_string(&composition_value).unwrap(),
            })
            .await
            .unwrap();
        let admitted =
            MapAuthoringReadScope::new("hydration-policy", "operations", vec!["sensitive".into()])
                .unwrap();
        let denied = MapAuthoringReadScope::new("hydration-policy", "operations", vec![]).unwrap();
        let foreign =
            MapAuthoringReadScope::new("hydration-foreign", "operations", vec!["sensitive".into()])
                .unwrap();
        assert_eq!(
            layer(layer_row.clone(), "hydration-policy").unwrap(),
            layer_value
        );
        assert_eq!(
            composition(composition_row.clone(), "hydration-policy").unwrap(),
            composition_value
        );
        for (is_layer, native_id, original) in [
            (true, layer_row.id.clone(), layer_row.canonical_json.clone()),
            (
                false,
                composition_row.id.clone(),
                composition_row.canonical_json.clone(),
            ),
        ] {
            for (field, changed) in [
                ("data_labels", serde_json::json!([])),
                ("classification", serde_json::Value::Null),
                ("created_by", serde_json::json!("other-author")),
                (
                    "owner",
                    serde_json::to_value(AccessSubject::Principal(
                        PrincipalId::parse("other-author").unwrap(),
                    ))
                    .unwrap(),
                ),
            ] {
                let mut body: serde_json::Value = serde_json::from_str(&original).unwrap();
                body[field] = changed;
                let corrupted = serde_json::to_string(&body).unwrap();
                db.a.client()
                    .query(include_str!(
                        "../queries/catalog/owned/tests/corrupt_route_document.surql"
                    ))
                    .bind(("route", native_id.clone()))
                    .bind(("canonical_json", corrupted.clone()))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
                if is_layer {
                    let selected = repository
                        .map_feature_layer(&admitted, &layer_id)
                        .await
                        .unwrap()
                        .unwrap();
                    assert_eq!(selected.canonical_json, corrupted);
                    assert!(layer(selected, "hydration-policy").is_err());
                    for excluded in [&denied, &foreign] {
                        assert!(
                            repository
                                .map_feature_layer(excluded, &layer_id)
                                .await
                                .unwrap()
                                .is_none()
                        );
                    }
                } else {
                    let selected = repository
                        .map_composition(&admitted, &composition_id)
                        .await
                        .unwrap()
                        .unwrap();
                    assert_eq!(selected.canonical_json, corrupted);
                    assert!(composition(selected, "hydration-policy").is_err());
                    for excluded in [&denied, &foreign] {
                        assert!(
                            repository
                                .map_composition(excluded, &composition_id)
                                .await
                                .unwrap()
                                .is_none()
                        );
                    }
                }
                db.a.client()
                    .query(include_str!(
                        "../queries/catalog/owned/tests/corrupt_route_document.surql"
                    ))
                    .bind(("route", native_id.clone()))
                    .bind(("canonical_json", original.clone()))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            }
        }
        assert_eq!(
            repository
                .map_feature_layer(&admitted, &layer_id)
                .await
                .unwrap()
                .unwrap(),
            layer_row
        );
        assert_eq!(
            repository
                .map_composition(&admitted, &composition_id)
                .await
                .unwrap()
                .unwrap(),
            composition_row
        );
    }
}
