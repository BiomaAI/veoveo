//! Lower checked resource selection to bound SQL data, never interpolated syntax.
use super::Document;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use veoveo_mcp_knowledge_extension::CollectionId;
use veoveo_types::{ResourceScheme, ResourceSelection, ResourceSelector, ResourceUriPrefix};

#[derive(Serialize, Deserialize)]
pub(super) struct SelectionBinding {
    collection_id: CollectionId,
    scheme: ResourceScheme,
    selectors: Vec<SelectorBinding>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SelectorBinding {
    Scheme { scheme: ResourceScheme },
    Prefix { prefix: ResourceUriPrefix },
    Template { literals: Vec<String> },
}
pub(super) fn bindings(
    selections: &BTreeMap<CollectionId, ResourceSelection>,
) -> Vec<Document<SelectionBinding>> {
    selections
        .iter()
        .map(|(collection, selection)| {
            Document(SelectionBinding {
                collection_id: collection.clone(),
                scheme: selection.scheme.clone(),
                selectors: selection
                    .selectors
                    .iter()
                    .map(|selector| match selector {
                        ResourceSelector::Scheme { scheme } => SelectorBinding::Scheme {
                            scheme: scheme.clone(),
                        },
                        ResourceSelector::UriPrefix { prefix } => SelectorBinding::Prefix {
                            prefix: prefix.clone(),
                        },
                        ResourceSelector::Template { uri_template } => SelectorBinding::Template {
                            literals: uri_template
                                .literal_segments()
                                .into_iter()
                                .map(str::to_owned)
                                .collect(),
                        },
                    })
                    .collect(),
            })
        })
        .collect()
}
