//! Negotiation and validation for clients, gateways and conformance consumers.
use crate::{
    CollectionDescriptor, EXTENSION_ID, KnowledgeError, OBSERVATION_KEY, Observation,
    ReadCondition, Revision, content_digest,
};
use rmcp::model::{
    ClientCapabilities, ReadResourceResult, RequestMetaObject, ResourceContents, ResourceTemplate,
    ServerCapabilities,
};
use veoveo_types::ResourceUri;

pub fn declare(capabilities: &mut ClientCapabilities) {
    capabilities
        .extensions
        .get_or_insert_default()
        .insert(EXTENSION_ID.into(), Default::default());
}

pub fn supports(capabilities: &ServerCapabilities) -> bool {
    capabilities
        .extensions
        .as_ref()
        .and_then(|c| c.get(EXTENSION_ID))
        .is_some_and(|v| v.is_empty())
}

pub fn declare_read(meta: &mut RequestMetaObject, revision: Option<&Revision>) {
    let mut capabilities = meta.client_capabilities().unwrap_or_default();
    declare(&mut capabilities);
    meta.set_client_capabilities(capabilities);
    match revision {
        Some(revision) => {
            meta.insert(
                EXTENSION_ID.into(),
                serde_json::to_value(ReadCondition {
                    if_none_match: revision.clone(),
                })
                .expect("typed condition serializes"),
            );
        }
        None => {
            meta.remove(EXTENSION_ID);
        }
    }
}

pub fn collection(
    template: &ResourceTemplate,
) -> Result<Option<CollectionDescriptor>, KnowledgeError> {
    template
        .meta
        .as_ref()
        .and_then(|m| m.get(EXTENSION_ID))
        .map(|v| {
            serde_json::from_value(v.clone())
                .map_err(|_| KnowledgeError("invalid collection descriptor"))
        })
        .transpose()
}

pub fn observation(result: &ReadResourceResult) -> Result<Option<Observation>, KnowledgeError> {
    result
        .meta
        .as_ref()
        .and_then(|m| m.get(OBSERVATION_KEY))
        .map(|v| {
            serde_json::from_value(v.clone())
                .map_err(|_| KnowledgeError("invalid knowledge observation"))
        })
        .transpose()
}

/// Check content binding as well as wire shape before admitting provenance.
pub fn validate_read(
    result: &ReadResourceResult,
    uri: &ResourceUri,
    previous_revision: Option<&Revision>,
) -> Result<Option<Observation>, KnowledgeError> {
    let Some(observation) = observation(result)? else {
        return Ok(None);
    };
    if observation.not_modified() {
        if !result.contents.is_empty() || previous_revision != Some(observation.revision()) {
            return Err(KnowledgeError("invalid not-modified knowledge response"));
        }
    } else {
        let [
            ResourceContents::TextResourceContents {
                uri: item_uri,
                text,
                ..
            },
        ] = result.contents.as_slice()
        else {
            return Err(KnowledgeError(
                "knowledge member requires exactly one text item",
            ));
        };
        if item_uri != uri.as_str() || content_digest(text) != *observation.content_sha256() {
            return Err(KnowledgeError(
                "knowledge observation does not match returned content",
            ));
        }
    }
    Ok(Some(observation))
}
