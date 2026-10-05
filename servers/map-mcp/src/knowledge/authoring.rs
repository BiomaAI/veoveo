//! One database statement selects each body and its current layer access together.
use super::*;
use crate::persistence::{MapFeatureHeadRecord, MapFeatureLayerRecord, MapLayerPublicationRecord};
use veoveo_mcp_knowledge_extension::ReadPolicy;
use veoveo_platform_store::{ArtifactGrantSubjectKind, deterministic_work_context_id};
use veoveo_types::{AccessLevel, AccessSubject};

pub(super) async fn select(
    catalog: &MapCatalog,
    identity: &GatewayInternalIdentity,
    scope: &MapAccessContext,
    collection: MapKnowledgeCollection,
    exact: Option<&MapKnowledgeMember>,
    after: Option<&MapKnowledgeMember>,
) -> Result<Vec<ObservedMap>> {
    ensure!(
        identity
            .authority
            .artifact_access()
            .allows(AccessLevel::Read),
        "Work Context read membership required"
    );
    let (position, layer) = match (collection, exact.or(after)) {
        (MapKnowledgeCollection::Layers, address) => (
            address.map(|v| match v {
                MapKnowledgeMember::Layer { layer } => layer.as_str(),
                _ => unreachable!(),
            }),
            None,
        ),
        (MapKnowledgeCollection::Features, address) => (
            address.map(|v| match v {
                MapKnowledgeMember::Feature { feature, .. } => feature.as_str(),
                _ => unreachable!(),
            }),
            address.map(|v| match v {
                MapKnowledgeMember::Feature { layer, .. } => layer.as_str(),
                _ => unreachable!(),
            }),
        ),
        (MapKnowledgeCollection::Publications, address) => (
            address.map(|v| match v {
                MapKnowledgeMember::Publication { publication, .. } => publication.as_str(),
                _ => unreachable!(),
            }),
            address.map(|v| match v {
                MapKnowledgeMember::Publication { layer, .. } => layer.as_str(),
                _ => unreachable!(),
            }),
        ),
        _ => anyhow::bail!("invalid authoring collection"),
    };
    let sql = match (collection, exact.is_some(), after.is_some()) {
        (MapKnowledgeCollection::Layers, true, _) => {
            include_str!("../queries/knowledge/authoring/layers_exact.surql")
        }
        (MapKnowledgeCollection::Layers, false, true) => {
            include_str!("../queries/knowledge/authoring/layers_after.surql")
        }
        (MapKnowledgeCollection::Layers, false, false) => {
            include_str!("../queries/knowledge/authoring/layers_first.surql")
        }
        (MapKnowledgeCollection::Features, true, _) => {
            include_str!("../queries/knowledge/authoring/features_exact.surql")
        }
        (MapKnowledgeCollection::Features, false, true) => {
            include_str!("../queries/knowledge/authoring/features_after.surql")
        }
        (MapKnowledgeCollection::Features, false, false) => {
            include_str!("../queries/knowledge/authoring/features_first.surql")
        }
        (MapKnowledgeCollection::Publications, true, _) => {
            include_str!("../queries/knowledge/authoring/publications_exact.surql")
        }
        (MapKnowledgeCollection::Publications, false, true) => {
            include_str!("../queries/knowledge/authoring/publications_after.surql")
        }
        (MapKnowledgeCollection::Publications, false, false) => {
            include_str!("../queries/knowledge/authoring/publications_first.surql")
        }
        _ => anyhow::bail!("invalid authoring collection"),
    };
    let mut response = catalog
        .store()
        .client()
        .query(sql)
        .bind(("tenant", scope.identity.tenant_id.record_id()))
        .bind(("tenant_key", scope.identity.tenant_key.clone()))
        .bind((
            "context",
            deterministic_work_context_id(
                &scope.identity.tenant_key,
                identity.authority.work_context.as_str(),
            )?
            .record_id(),
        ))
        .bind((
            "labels",
            identity
                .actor
                .data_labels
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        ))
        .bind(("key", position.map(str::to_owned)))
        .bind(("layer", layer.map(str::to_owned)))
        .bind((
            "limit",
            if exact.is_some() {
                2usize
            } else {
                PAGE_SIZE + 1
            },
        ))
        .await?
        .check()?;
    match collection {
        MapKnowledgeCollection::Layers => {
            let rows: Vec<MapFeatureLayerRecord> = response.take(0)?;
            rows.into_iter()
                .map(|row| {
                    let (layer, access) = checked_layer(row, identity, scope)?;
                    ObservedMap::new(
                        MapKnowledgeMember::Layer {
                            layer: layer.layer_id.clone(),
                        },
                        layer.title.clone(),
                        &layer,
                        Some(access),
                        Some(layer.updated_at),
                        None,
                    )
                })
                .collect()
        }
        MapKnowledgeCollection::Features => {
            let rows: Vec<(MapFeatureHeadRecord, MapFeatureLayerRecord)> = response.take(0)?;
            rows.into_iter()
                .map(|(row, parent)| {
                    let (layer, access) = checked_layer(parent, identity, scope)?;
                    let feature: MapFeature = serde_json::from_str(&row.canonical_json)?;
                    feature.geometry.validate()?;
                    ensure!(
                        feature.layer_id == layer.layer_id
                            && row.layer_key == layer.layer_id.as_str()
                            && row.feature_key == feature.id.as_str()
                            && row.feature_revision == i64::try_from(feature.feature_revision)?
                            && row.layer_revision == i64::try_from(feature.layer_revision)?
                            && row.deleted == feature.deleted
                            && feature.provenance.work_context == layer.work_context,
                        "feature document disagrees with selected identity or revision"
                    );
                    ObservedMap::new(
                        MapKnowledgeMember::Feature {
                            layer: feature.layer_id.clone(),
                            feature: feature.id.clone(),
                        },
                        feature
                            .title
                            .clone()
                            .unwrap_or_else(|| feature.id.to_string()),
                        &feature,
                        Some(access),
                        Some(feature.created_at),
                        Some(ModifiedBy::Principal(feature.provenance.actor_id.clone())),
                    )
                })
                .collect()
        }
        MapKnowledgeCollection::Publications => {
            let rows: Vec<(MapLayerPublicationRecord, MapFeatureLayerRecord)> = response.take(0)?;
            rows.into_iter()
                .map(|(row, parent)| {
                    let (layer, access) = checked_layer(parent, identity, scope)?;
                    let publication: LayerPublication = serde_json::from_str(&row.canonical_json)?;
                    ensure!(
                        publication.layer_id == layer.layer_id
                            && row.layer_key == layer.layer_id.as_str()
                            && row.publication_key == publication.publication_id.as_str()
                            && row.layer_revision == i64::try_from(publication.layer_revision)?
                            && row.published_by_key == publication.published_by.as_str()
                            && publication.work_context == layer.work_context,
                        "publication document disagrees with selected identity or attribution"
                    );
                    ObservedMap::new(
                        MapKnowledgeMember::Publication {
                            layer: publication.layer_id.clone(),
                            publication: publication.publication_id.clone(),
                        },
                        publication
                            .title
                            .clone()
                            .unwrap_or_else(|| publication.publication_id.to_string()),
                        &publication,
                        Some(access),
                        Some(publication.published_at),
                        Some(ModifiedBy::Principal(publication.published_by.clone())),
                    )
                })
                .collect()
        }
        _ => unreachable!("authoring collection checked above"),
    }
}

fn checked_layer(
    row: MapFeatureLayerRecord,
    identity: &GatewayInternalIdentity,
    scope: &MapAccessContext,
) -> Result<(FeatureLayer, AccessDescriptor)> {
    let layer: FeatureLayer = serde_json::from_str(&row.canonical_json)?;
    let owner = match row.owner_kind {
        ArtifactGrantSubjectKind::Principal => AccessSubject::Principal(row.owner_key.parse()?),
        ArtifactGrantSubjectKind::Group => AccessSubject::Group(row.owner_key.parse()?),
    };
    ensure!(
        row.tenant == scope.identity.tenant_id.record_id()
            && row.work_context
                == deterministic_work_context_id(
                    &scope.identity.tenant_key,
                    layer.work_context.as_str()
                )?
                .record_id()
            && layer.work_context == identity.authority.work_context
            && row.authority.context_key == layer.work_context.as_str()
            && layer.layer_id.as_str() == row.layer_key
            && layer.owner == owner
            && layer.created_by.as_str() == row.created_by_key
            && i64::try_from(layer.revision)? == row.revision
            && layer
                .data_labels
                .iter()
                .map(ToString::to_string)
                .collect::<std::collections::BTreeSet<_>>()
                == row.data_labels.iter().cloned().collect()
            && layer.classification.as_ref().map(ToString::to_string) == row.classification,
        "layer document disagrees with selected identity, revision or access"
    );
    let access = AccessDescriptor {
        tenant: scope.identity.tenant_key.parse()?,
        work_context: layer.work_context.clone(),
        read_policy: ReadPolicy::SelectedWorkContextMembers {},
        owner,
        grants: vec![],
        data_labels: layer.data_labels.iter().cloned().collect(),
        expires_at: None,
    };
    Ok((layer, access))
}
