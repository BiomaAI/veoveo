//! Gateway-only indexing intent. It requests a collection; installation policy
//! authenticates the machine client and the source observation confirms membership.
use crate::{CollectionDescriptor, CollectionId, KnowledgeError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use veoveo_types::{ResourceUri, ResourceUriBuilder, ResourceUriParts};

pub const INDEXING_READ_KEY: &str = "ai.veoveo/indexing-read";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IndexingReadKind {
    SourceContract,
    Enumeration,
    Member,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndexingReadIntent {
    pub collection: CollectionId,
    pub kind: IndexingReadKind,
}

pub fn enumeration_uri(
    descriptor: &CollectionDescriptor,
    cursor: Option<&str>,
) -> Result<ResourceUri, KnowledgeError> {
    if cursor.is_some_and(|c| c.is_empty() || c.len() > 4096 || c.chars().any(char::is_control)) {
        return Err(KnowledgeError("invalid enumeration cursor"));
    }
    let mut parameters = BTreeMap::new();
    if let Some(cursor) = cursor {
        parameters.insert("cursor".to_owned(), cursor.to_owned());
    }
    let uri = descriptor
        .enumerate()
        .expand_scalars(&parameters)
        .map_err(|_| KnowledgeError("invalid enumeration template"))?;
    if let Some(cursor) = cursor {
        let parts = ResourceUriParts::parse(uri.as_str())
            .map_err(|_| KnowledgeError("invalid enumeration address"))?;
        if !parts.query_parameters().contains_key("cursor") {
            return ResourceUriBuilder::from_parts(parts)
                .query_pair("cursor", cursor)
                .and_then(|builder| builder.build())
                .map_err(|_| KnowledgeError("enumeration must declare a cursor parameter"));
        }
        if parts.query_parameters().get("cursor").map(String::as_str) != Some(cursor) {
            return Err(KnowledgeError(
                "enumeration template does not bind the requested cursor",
            ));
        }
    }
    Ok(uri)
}

pub fn is_enumeration_uri(descriptor: &CollectionDescriptor, uri: &ResourceUri) -> bool {
    let Ok(parts) = uri.components() else {
        return false;
    };
    enumeration_uri(
        descriptor,
        parts.query_parameters().get("cursor").map(String::as_str),
    )
    .is_ok_and(|expected| expected == *uri)
}
