use crate::ServiceError;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, future::Future};
use veoveo_knowledge_contract::{KnowledgeError, MemberTitle};
use veoveo_mcp_knowledge_extension::{CollectionDescriptor, Observation, content_digest};
use veoveo_types::{ResourceUri, ResourceUriParts};
mod gateway;
pub use gateway::GatewaySource;

/// Owners may add fields to enumeration items. This reader consumes the shared
/// URI/title projection; typed source-specific fields stay with their owner.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemberLink {
    pub uri: ResourceUri,
    #[serde(default)]
    pub title: Option<MemberTitle>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(try_from = "PageWire", into = "PageWire")]
pub struct SourcePage(PageWire);
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PageWire {
    items: Vec<MemberLink>,
    next_cursor: Option<String>,
}
impl SourcePage {
    pub fn new(
        items: Vec<MemberLink>,
        next_cursor: Option<String>,
    ) -> Result<Self, KnowledgeError> {
        PageWire { items, next_cursor }.try_into()
    }
    pub fn items(&self) -> &[MemberLink] {
        &self.0.items
    }
    pub fn next_cursor(&self) -> Option<&str> {
        self.0.next_cursor.as_deref()
    }
}
impl TryFrom<PageWire> for SourcePage {
    type Error = KnowledgeError;
    fn try_from(page: PageWire) -> Result<Self, Self::Error> {
        if page.items.len() > 100
            || page
                .items
                .iter()
                .map(|m| &m.uri)
                .collect::<BTreeSet<_>>()
                .len()
                != page.items.len()
            || page
                .next_cursor
                .as_ref()
                .is_some_and(|c| c.is_empty() || c.len() > 4096 || c.chars().any(char::is_control))
            || (page.items.is_empty() && page.next_cursor.is_some())
        {
            return Err(KnowledgeError("invalid knowledge enumeration page"));
        }
        for item in &page.items {
            ResourceUriParts::parse(item.uri.as_str())
                .map_err(|_| KnowledgeError("enumeration requires concrete member URIs"))?;
        }
        Ok(Self(page))
    }
}
impl From<SourcePage> for PageWire {
    fn from(page: SourcePage) -> Self {
        page.0
    }
}

pub struct SourceDocument {
    text: String,
    observation: Observation,
}
impl SourceDocument {
    pub fn new(text: String, observation: Observation) -> Result<Self, KnowledgeError> {
        if text.len() > 64 * 1024
            || observation.not_modified()
            || content_digest(&text) != *observation.content_sha256()
        {
            return Err(KnowledgeError(
                "source content does not match its observation",
            ));
        }
        Ok(Self { text, observation })
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn observation(&self) -> &Observation {
        &self.observation
    }
}

/// The adapter must use the gateway's authenticated knowledge read path and
/// enforce the requested resource URI. Failures include unqualified not-found;
/// they never certify deletion. Each call runs under the indexer's deadline.
pub trait KnowledgeSource: Send + Sync {
    fn enumerate(
        &self,
        collection: &CollectionDescriptor,
        uri: ResourceUri,
    ) -> impl Future<Output = Result<SourcePage, ServiceError>> + Send;
    fn read(
        &self,
        collection: &CollectionDescriptor,
        uri: ResourceUri,
    ) -> impl Future<Output = Result<SourceDocument, ServiceError>> + Send;
}

pub use veoveo_mcp_knowledge_extension::enumeration_uri;
