//! MCP adapters. Callers must authorize before constructing a member response.
use crate::{
    CollectionDescriptor, EXTENSION_ID, KnowledgeError, OBSERVATION_KEY, Observation,
    ReadCondition, SearchDeclaration, content_digest,
};
use rmcp::model::{
    MetaObject, ReadResourceResult, RequestMetaObject, ResourceContents, ResourceTemplate,
    ServerCapabilities, Tool,
};
use veoveo_types::ResourceUri;

pub fn declare(capabilities: &mut ServerCapabilities) {
    capabilities
        .extensions
        .get_or_insert_default()
        .insert(EXTENSION_ID.into(), Default::default());
}

pub fn attach_collection(template: &mut ResourceTemplate, collection: &CollectionDescriptor) {
    template.meta.get_or_insert_default().insert(
        EXTENSION_ID.into(),
        serde_json::to_value(collection).expect("typed collection serializes"),
    );
}

pub fn attach_search(tool: &mut Tool, declaration: &SearchDeclaration) {
    tool.meta.get_or_insert_default().insert(
        EXTENSION_ID.into(),
        serde_json::to_value(declaration).expect("typed search declaration serializes"),
    );
}

pub fn requested(meta: Option<&RequestMetaObject>) -> Result<bool, KnowledgeError> {
    let Some(capabilities) = meta.and_then(RequestMetaObject::client_capabilities) else {
        return Ok(false);
    };
    let Some(settings) = capabilities
        .extensions
        .as_ref()
        .and_then(|c| c.get(EXTENSION_ID))
    else {
        return Ok(false);
    };
    if !settings.is_empty() {
        return Err(KnowledgeError("unsupported knowledge-source settings"));
    }
    Ok(true)
}

pub fn condition(
    meta: Option<&RequestMetaObject>,
) -> Result<Option<ReadCondition>, KnowledgeError> {
    if !requested(meta)? {
        return Ok(None);
    }
    meta.and_then(|m| m.get(EXTENSION_ID))
        .map(|v| {
            serde_json::from_value(v.clone())
                .map_err(|_| KnowledgeError("invalid knowledge read condition"))
        })
        .transpose()
}

/// One admitted text member has exactly one observation. Conditional evaluation
/// happens here, after domain authorization and validation of the current bytes.
pub fn member_result(
    uri: &ResourceUri,
    mime_type: &str,
    text: String,
    mut observation: Observation,
    collection: &CollectionDescriptor,
    request_meta: Option<&RequestMetaObject>,
) -> Result<ReadResourceResult, KnowledgeError> {
    observation.validate_collection(collection)?;
    if observation.not_modified() || observation.content_sha256() != &content_digest(&text) {
        return Err(KnowledgeError("member bytes disagree with observation"));
    }
    let negotiated = requested(request_meta)?;
    let not_modified =
        condition(request_meta)?.is_some_and(|c| &c.if_none_match == observation.revision());
    let contents = if not_modified {
        Vec::new()
    } else {
        vec![ResourceContents::text(text, uri.as_str()).with_mime_type(mime_type)]
    };
    let mut result = ReadResourceResult::new(contents);
    if negotiated {
        observation.set_not_modified(not_modified);
        let mut meta = MetaObject::default();
        meta.insert(
            OBSERVATION_KEY.into(),
            serde_json::to_value(observation).expect("typed observation serializes"),
        );
        result.meta = Some(meta);
    }
    Ok(result)
}
