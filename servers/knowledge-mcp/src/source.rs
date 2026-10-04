use crate::ServiceError;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, future::Future};
use veoveo_knowledge_contract::{KnowledgeError, MemberTitle};
use veoveo_mcp_knowledge_extension::{CollectionDescriptor, Observation, content_digest};
use veoveo_types::{ResourceUri, ResourceUriParts};
mod discovery;
mod gateway;
pub use discovery::{
    ApprovedCollection, DiscoveredCollections, DiscoveryScope, GatewayCatalogListener,
};
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
pub struct SourcePage(veoveo_types::Checked<PageWire>);
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
impl veoveo_types::Check for PageWire {
    type Error = KnowledgeError;
    fn check(&self) -> Result<(), Self::Error> {
        let page = self;
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
        Ok(())
    }
}
impl TryFrom<PageWire> for SourcePage {
    type Error = KnowledgeError;
    fn try_from(value: PageWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<SourcePage> for PageWire {
    fn from(page: SourcePage) -> Self {
        page.0.into_inner()
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

pub enum SourceRead {
    Modified(SourceDocument),
    NotModified(Observation),
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
        previous: Option<&Observation>,
    ) -> impl Future<Output = Result<SourceRead, ServiceError>> + Send;
}

/// `listen` acknowledges the collection root before returning. Every member
/// content/access change and enumeration change must invalidate this listener.
pub trait ObservableSource: KnowledgeSource {
    type Listener: SourceListener + Send + 'static;
    fn listen(
        &self,
        collection: &CollectionDescriptor,
    ) -> impl Future<Output = Result<Self::Listener, ServiceError>> + Send;
}

/// A stream gap or termination is an error and requires a new listener followed
/// by complete enumeration. No source failure certifies deletion.
pub trait SourceListener {
    fn changed(&mut self) -> impl Future<Output = Result<(), ServiceError>> + Send;
}

pub use veoveo_mcp_knowledge_extension::enumeration_uri;
