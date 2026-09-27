//! Authoring completion shares the read predicates and returns only matching keys.
use super::{CHILD_VISIBLE, COMPOSITION_VISIBLE, LAYER_VISIBLE, MapAuthoringReadScope};
use crate::{PlatformStore, StoreError};

#[derive(Clone, Debug)]
pub enum MapAuthoringCompletion {
    Layer,
    SchemaVersion {
        layer: Option<String>,
    },
    StyleVersion {
        layer: Option<String>,
    },
    StyleRevision,
    Publication {
        layer: Option<String>,
    },
    Product {
        layer: Option<String>,
        publication: Option<String>,
    },
    Composition,
    CompositionRevision {
        composition: Option<String>,
    },
}

impl PlatformStore {
    pub async fn complete_map_authoring(
        &self,
        scope: &MapAuthoringReadScope,
        domain: MapAuthoringCompletion,
        needle: &str,
    ) -> Result<Vec<String>, StoreError> {
        crate::map::validate_completion_needle(needle)?;
        let (table, field, visible, parent_field, parent, publication) = match domain {
            MapAuthoringCompletion::Layer => (
                "map_feature_layer",
                "layer_key",
                LAYER_VISIBLE,
                "layer_key",
                None,
                None,
            ),
            MapAuthoringCompletion::SchemaVersion { layer } => (
                "map_feature_schema_revision",
                "type::string(schema_version)",
                CHILD_VISIBLE,
                "layer_key",
                layer,
                None,
            ),
            MapAuthoringCompletion::StyleVersion { layer } => (
                "map_style_revision",
                "type::string(style_version)",
                CHILD_VISIBLE,
                "layer_key",
                layer,
                None,
            ),
            MapAuthoringCompletion::StyleRevision => (
                "map_style_revision",
                "style_revision_key",
                CHILD_VISIBLE,
                "layer_key",
                None,
                None,
            ),
            MapAuthoringCompletion::Publication { layer } => (
                "map_layer_publication",
                "publication_key",
                CHILD_VISIBLE,
                "layer_key",
                layer,
                None,
            ),
            MapAuthoringCompletion::Product { layer, publication } => (
                "map_layer_product",
                "product_key",
                CHILD_VISIBLE,
                "layer_key",
                layer,
                publication,
            ),
            MapAuthoringCompletion::Composition => (
                "map_composition",
                "composition_key",
                COMPOSITION_VISIBLE,
                "composition_key",
                None,
                None,
            ),
            MapAuthoringCompletion::CompositionRevision { composition } => (
                "map_composition_revision",
                "type::string(revision)",
                "tenant = $tenant AND work_context = $context AND composition_key IN (SELECT VALUE composition_key FROM map_composition WHERE tenant = $tenant AND work_context = $context AND $labels CONTAINSALL authority.data_labels)",
                "composition_key",
                composition,
                None,
            ),
        };
        let publication_predicate = if publication.is_some() {
            " AND publication_key = $publication"
        } else {
            ""
        };
        let sql = format!(
            "SELECT VALUE candidate FROM (SELECT {field} AS candidate FROM {table} WHERE {visible} AND ($parent = NONE OR {parent_field} = $parent){publication_predicate} AND string::lowercase({field}) CONTAINS $needle GROUP BY candidate ORDER BY candidate ASC LIMIT 101);"
        );
        let mut response = self
            .client()
            .query(sql)
            .bind(("tenant", scope.tenant.clone()))
            .bind(("context", scope.context.clone()))
            .bind(("labels", scope.labels.clone()))
            .bind(("parent", parent))
            .bind(("publication", publication))
            .bind(("needle", needle.to_lowercase()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}
