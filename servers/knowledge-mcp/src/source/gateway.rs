//! Reuses an authenticated gateway connection owned by the service coordinator.
use super::{KnowledgeSource, SourceDocument, SourcePage, SourceRead};
use crate::ServiceError;
use rmcp::{
    Peer, RoleClient,
    model::{
        ClientCapabilities, ReadResourceRequestParams, ReadResourceResult, ResourceContents,
        ServerResult,
    },
    service::PeerRequestOptions,
};
use veoveo_knowledge_contract::KnowledgeError;
use veoveo_mcp_knowledge_extension::{
    CollectionDescriptor, INDEXING_READ_KEY, IndexingReadIntent, IndexingReadKind, client,
};
use veoveo_types::ResourceUri;

pub struct GatewaySource {
    peer: Peer<RoleClient>,
}
impl GatewaySource {
    /// The caller owns machine-client authentication, profile selection and the
    /// connection lifetime. This adapter never connects to source URI hosts.
    pub fn from_authenticated_peer(peer: Peer<RoleClient>) -> Self {
        Self { peer }
    }

    async fn read_result(
        &self,
        collection: &CollectionDescriptor,
        kind: IndexingReadKind,
        uri: &ResourceUri,
        previous: Option<&veoveo_mcp_knowledge_extension::Observation>,
    ) -> Result<ReadResourceResult, ServiceError> {
        let (request, mut options) = client::read_request(
            ReadResourceRequestParams::new(uri.as_str()),
            ClientCapabilities::default(),
            previous.map(|observation| observation.revision()),
            PeerRequestOptions::default(),
        );
        options.meta.get_or_insert_default().insert(
            INDEXING_READ_KEY.into(),
            serde_json::to_value(IndexingReadIntent {
                collection: collection.collection().clone(),
                kind,
            })
            .expect("typed indexing intent"),
        );
        let result = self
            .peer
            .send_request_with_option(request, options)
            .await
            .map_err(|_| ServiceError::SourceUnavailable)?
            .await_response()
            .await
            .map_err(|_| ServiceError::SourceUnavailable)?;
        match result {
            ServerResult::ReadResourceResult(result) => Ok(result),
            _ => Err(KnowledgeError("source requires a terminal resource response").into()),
        }
    }
}
impl KnowledgeSource for GatewaySource {
    async fn enumerate(
        &self,
        collection: &CollectionDescriptor,
        uri: ResourceUri,
    ) -> Result<SourcePage, ServiceError> {
        let result = self
            .read_result(collection, IndexingReadKind::Enumeration, &uri, None)
            .await?;
        let text = text(&result, &uri, 512 * 1024)?;
        serde_json::from_str(text)
            .map_err(|_| KnowledgeError("invalid source enumeration response").into())
    }
    async fn read(
        &self,
        collection: &CollectionDescriptor,
        uri: ResourceUri,
        previous: Option<&veoveo_mcp_knowledge_extension::Observation>,
    ) -> Result<SourceRead, ServiceError> {
        let result = self
            .read_result(collection, IndexingReadKind::Member, &uri, previous)
            .await?;
        let observation = client::validate_read(
            &result,
            &uri,
            previous.map(|observation| observation.revision()),
        )?
        .ok_or(KnowledgeError("source omitted its knowledge observation"))?;
        if observation.not_modified() {
            let prior = previous.ok_or(KnowledgeError("unexpected conditional response"))?;
            observation.revalidated(prior)?;
            return Ok(SourceRead::NotModified(observation));
        }
        let text = text(&result, &uri, 64 * 1024)?;
        Ok(SourceRead::Modified(SourceDocument::new(
            text.to_owned(),
            observation,
        )?))
    }
}
fn text<'a>(
    result: &'a ReadResourceResult,
    uri: &ResourceUri,
    cap: usize,
) -> Result<&'a str, ServiceError> {
    if let [
        ResourceContents::TextResourceContents {
            uri: returned,
            text,
            ..
        },
    ] = result.contents.as_slice()
        && returned == uri.as_str()
        && text.len() <= cap
    {
        return Ok(text);
    }
    Err(KnowledgeError("source requires one URI-matching text item within its byte cap").into())
}
