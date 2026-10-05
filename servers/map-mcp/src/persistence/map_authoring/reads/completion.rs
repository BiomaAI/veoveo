//! Authoring completion shares the read predicates and returns only matching keys.
use super::MapAuthoringReadScope;
use crate::persistence::{MapRepository, MapStoreError};

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

impl MapRepository {
    pub async fn complete_map_authoring(
        &self,
        scope: &MapAuthoringReadScope,
        domain: MapAuthoringCompletion,
        needle: &str,
    ) -> Result<Vec<String>, MapStoreError> {
        crate::persistence::map::validate_completion_needle(needle)?;
        let (sql, parent, publication) = match domain {
            MapAuthoringCompletion::Layer => (
                include_str!("../../queries/map_authoring/reads/completion/layer.surql"),
                None,
                None,
            ),
            MapAuthoringCompletion::SchemaVersion { layer } => (
                include_str!("../../queries/map_authoring/reads/completion/schema_version.surql"),
                layer,
                None,
            ),
            MapAuthoringCompletion::StyleVersion { layer } => (
                include_str!("../../queries/map_authoring/reads/completion/style_version.surql"),
                layer,
                None,
            ),
            MapAuthoringCompletion::StyleRevision => (
                include_str!("../../queries/map_authoring/reads/completion/style_revision.surql"),
                None,
                None,
            ),
            MapAuthoringCompletion::Publication { layer } => (
                include_str!("../../queries/map_authoring/reads/completion/publication.surql"),
                layer,
                None,
            ),
            MapAuthoringCompletion::Product { layer, publication } => (
                include_str!("../../queries/map_authoring/reads/completion/product.surql"),
                layer,
                publication,
            ),
            MapAuthoringCompletion::Composition => (
                include_str!("../../queries/map_authoring/reads/completion/composition.surql"),
                None,
                None,
            ),
            MapAuthoringCompletion::CompositionRevision { composition } => (
                include_str!(
                    "../../queries/map_authoring/reads/completion/composition_revision.surql"
                ),
                composition,
                None,
            ),
        };
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
