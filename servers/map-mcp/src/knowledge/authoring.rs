//! One database statement selects each body and its current layer access together.
use super::*;
use veoveo_mcp_knowledge_extension::ReadPolicy;
use veoveo_platform_store::{
    ArtifactGrantSubjectKind, MapFeatureHeadRecord, MapFeatureLayerRecord,
    MapLayerPublicationRecord, deterministic_work_context_id,
};
use veoveo_types::{AccessLevel, AccessSubject};

const VISIBLE: &str =
    "tenant = $tenant AND work_context = $context AND $labels CONTAINSALL data_labels";

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
    let (table, key, order, position, parent, layer) = match (collection, exact.or(after)) {
        (MapKnowledgeCollection::Layers, address) => (
            "map_feature_layer",
            "layer_key",
            "layer_key",
            address.map(|v| match v {
                MapKnowledgeMember::Layer { layer } => layer.as_str(),
                _ => unreachable!(),
            }),
            None,
            None,
        ),
        (MapKnowledgeCollection::Features, address) => (
            "map_feature_head",
            "feature_key",
            "layer_key, feature_key",
            address.map(|v| match v {
                MapKnowledgeMember::Feature { feature, .. } => feature.as_str(),
                _ => unreachable!(),
            }),
            Some("feature"),
            address.map(|v| match v {
                MapKnowledgeMember::Feature { layer, .. } => layer.as_str(),
                _ => unreachable!(),
            }),
        ),
        (MapKnowledgeCollection::Publications, address) => (
            "map_layer_publication",
            "publication_key",
            "publication_key",
            address.map(|v| match v {
                MapKnowledgeMember::Publication { publication, .. } => publication.as_str(),
                _ => unreachable!(),
            }),
            Some("publication"),
            address.map(|v| match v {
                MapKnowledgeMember::Publication { layer, .. } => layer.as_str(),
                _ => unreachable!(),
            }),
        ),
        _ => anyhow::bail!("invalid authoring collection"),
    };
    let selection = if exact.is_some() {
        if parent.is_some() {
            format!("AND {key} = $key AND layer_key = $layer")
        } else {
            format!("AND {key} = $key")
        }
    } else if after.is_some() {
        if collection == MapKnowledgeCollection::Features {
            "AND (layer_key > $layer OR (layer_key = $layer AND feature_key > $key))".into()
        } else {
            format!("AND {key} > $key")
        }
    } else {
        String::new()
    };
    let predicate = if parent.is_some() {
        format!(
            "tenant = $tenant AND work_context = $context AND layer_key IN (SELECT VALUE layer_key FROM map_feature_layer WHERE {VISIBLE})"
        )
    } else {
        VISIBLE.into()
    };
    let select = format!(
        "SELECT * FROM {table} WHERE {predicate} {selection} ORDER BY {order} LIMIT $limit"
    );
    let sql = if parent.is_some() {
        format!(
            "RETURN ({select}).map(|$row| [$row, (SELECT * FROM ONLY type::record('map_feature_layer', [$tenant_key, $row.layer_key]) WHERE {VISIBLE})]);"
        )
    } else {
        format!("{select};")
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
