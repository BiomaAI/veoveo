//! Completion dispatch; persisted catalogs apply scope and matching in SQL.
use super::*;
use crate::{analytics::GeographyCompletion, catalog::MapAccessContext};
use veoveo_platform_store::{MapAuthoringCompletion as Authoring, MapCatalogCompletion as Catalog};

impl MapMcp {
    pub(super) async fn complete_index(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapAccessContext,
        template: &str,
        request: &CompleteRequestParams,
    ) -> Result<CompleteResult, McpError> {
        let needle = &request.argument.value;
        let argument = request.argument.name.as_str();
        let catalog = match (template, argument) {
            (uris::DATASET_TEMPLATE | uris::RELEASE_TEMPLATE, "dataset_id") => {
                Some(Catalog::Dataset)
            }
            (uris::RELEASE_TEMPLATE, "release_id") => Some(Catalog::Release {
                dataset: parent(request, "dataset_id")?,
            }),
            (uris::MOBILITY_PROFILE_TEMPLATE, "profile_id") => Some(Catalog::MobilityProfile),
            (uris::MOBILITY_PROFILE_TEMPLATE, "profile_version") => {
                Some(Catalog::MobilityProfileVersion {
                    profile: parent(request, "profile_id")?,
                })
            }
            (uris::ROUTE_TEMPLATE, "route_id") => Some(Catalog::Route),
            (uris::MATRIX_TEMPLATE, "matrix_id") => Some(Catalog::Matrix),
            _ => None,
        };
        if let Some(domain) = catalog {
            return finish(
                self.state
                    .catalog
                    .store()
                    .complete_map_catalog(&scope.identity, domain, needle)
                    .await
                    .map_err(internal)?,
            );
        }
        let authoring = match (template, argument) {
            (
                uris::FEATURE_LAYER_TEMPLATE
                | uris::PUBLICATIONS_PAGE_TEMPLATE
                | uris::FEATURE_SCHEMA_TEMPLATE
                | uris::FEATURE_STYLE_TEMPLATE
                | uris::FEATURES_TEMPLATE
                | uris::FEATURE_TEMPLATE
                | uris::FEATURE_REVISION_TEMPLATE
                | uris::CHANGESET_TEMPLATE
                | uris::PUBLICATION_TEMPLATE
                | uris::LAYER_PRODUCT_TEMPLATE,
                "layer_id",
            ) => Some(Authoring::Layer),
            (uris::FEATURE_SCHEMA_TEMPLATE, "schema_version") => Some(Authoring::SchemaVersion {
                layer: parent(request, "layer_id")?,
            }),
            (uris::FEATURE_STYLE_TEMPLATE, "style_version") => Some(Authoring::StyleVersion {
                layer: parent(request, "layer_id")?,
            }),
            (uris::FEATURE_STYLE_REVISION_TEMPLATE, "style_revision_id") => {
                Some(Authoring::StyleRevision)
            }
            (
                uris::PUBLICATION_TEMPLATE
                | uris::FEATURES_TEMPLATE
                | uris::LAYER_PRODUCT_TEMPLATE
                | uris::LAYER_PRODUCTS_PAGE_TEMPLATE,
                "publication_id",
            ) => Some(Authoring::Publication {
                layer: parent(request, "layer_id")?,
            }),
            (uris::LAYER_PRODUCT_TEMPLATE, "product_id") => Some(Authoring::Product {
                layer: parent(request, "layer_id")?,
                publication: parent(request, "publication_id")?,
            }),
            (
                uris::COMPOSITION_TEMPLATE | uris::COMPOSITION_REVISION_TEMPLATE,
                "composition_id",
            ) => Some(Authoring::Composition),
            (uris::COMPOSITION_REVISION_TEMPLATE, "composition_revision") => {
                Some(Authoring::CompositionRevision {
                    composition: parent(request, "composition_id")?,
                })
            }
            _ => None,
        };
        if let Some(domain) = authoring {
            return finish(
                self.state
                    .authoring
                    .complete(identity, scope, domain, needle)
                    .await
                    .map_err(internal)?,
            );
        }
        let values = match (template, argument) {
            (uris::LOCATION_TEMPLATE, "location_id") => self
                .state
                .analytics
                .complete_geography(&scope.tenant_key(), GeographyCompletion::Location, needle)
                .map_err(internal)?,
            (uris::FACILITY_TEMPLATE, "facility_id") => self
                .state
                .analytics
                .complete_geography(&scope.tenant_key(), GeographyCompletion::Facility, needle)
                .map_err(internal)?,
            (uris::SOURCE_TEMPLATE, "source_id") => self
                .state
                .catalog
                .complete_sources(scope, needle)
                .await
                .map_err(internal)?
                .into_iter()
                .map(|id| id.to_string())
                .collect(),
            (uris::RESTRICTION_TEMPLATE, "restriction_id") => self
                .state
                .catalog
                .complete_restrictions(scope, needle)
                .await
                .map_err(internal)?
                .into_iter()
                .map(|id| id.to_string())
                .collect(),
            (uris::TRAVEL_MODEL_TEMPLATE, "travel_model_id") => {
                crate::travel_models::TravelModelReads::new(self.state.catalog.store())
                    .complete(&crate::server::tasks::runtime_owner(identity), needle)
                    .await
                    .map_err(internal)?
                    .into_iter()
                    .map(|id| id.to_string())
                    .collect()
            }
            // Document IDs are a fixed, packaged inventory with no database rows.
            (uris::DOC_TEMPLATE, "doc_id") => {
                let needle = needle.to_lowercase();
                let mut values: Vec<_> = SERVER_DOCS
                    .iter()
                    .filter(|doc| doc.id.to_lowercase().contains(&needle))
                    .map(|doc| doc.id.to_owned())
                    .collect();
                values.sort();
                values
            }
            _ => vec![],
        };
        finish(values)
    }
}

fn parent(request: &CompleteRequestParams, name: &str) -> Result<Option<String>, McpError> {
    let Some(value) = request
        .context
        .as_ref()
        .and_then(|context| context.get_argument(name))
    else {
        return Ok(None);
    };
    let value = match name {
        "dataset_id" => MapDatasetId::parse(value.clone()).map(|id| id.to_string()),
        "profile_id" => MobilityProfileId::parse(value.clone()).map(|id| id.to_string()),
        "layer_id" => {
            crate::contract::FeatureLayerId::parse(value.clone()).map(|id| id.to_string())
        }
        "publication_id" => {
            crate::contract::LayerPublicationId::parse(value.clone()).map(|id| id.to_string())
        }
        "composition_id" => {
            crate::contract::MapCompositionId::parse(value.clone()).map(|id| id.to_string())
        }
        _ => unreachable!("closed completion parent mapping"),
    }
    .map_err(invalid_params)?;
    Ok(Some(value))
}

fn finish(mut values: Vec<String>) -> Result<CompleteResult, McpError> {
    let more = values.len() > CompletionInfo::MAX_VALUES;
    values.truncate(CompletionInfo::MAX_VALUES);
    let total = (!more).then_some(values.len() as u32);
    Ok(CompleteResult::new(
        CompletionInfo::with_pagination(values, total, more).map_err(internal)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{ArgumentInfo, CompletionContext};

    #[test]
    fn completion_reports_unknown_total_when_the_sql_page_has_more_matches() {
        let result = finish((0..101).map(|n| n.to_string()).collect())
            .unwrap()
            .completion;
        assert_eq!(result.values.len(), 100);
        assert_eq!(result.has_more, Some(true));
        assert_eq!(result.total, None);
        let result = finish(vec!["one".into()]).unwrap().completion;
        assert_eq!(result.has_more, Some(false));
        assert_eq!(result.total, Some(1));
    }
    #[test]
    fn completion_parent_arguments_are_typed_before_query_dispatch() {
        let layer = crate::contract::FeatureLayerId::new();
        let request = CompleteRequestParams::new(
            Reference::for_resource(uris::FEATURE_SCHEMA_TEMPLATE),
            ArgumentInfo::new("schema_version", ""),
        )
        .with_context(CompletionContext::with_arguments(
            std::collections::HashMap::from([("layer_id".into(), layer.to_string())]),
        ));
        assert_eq!(
            parent(&request, "layer_id").unwrap(),
            Some(layer.to_string())
        );
        assert_eq!(parent(&request, "publication_id").unwrap(), None);
        let request = request.with_context(CompletionContext::with_arguments(
            std::collections::HashMap::from([("layer_id".into(), "' OR true --".into())]),
        ));
        assert!(parent(&request, "layer_id").is_err());
    }
}
