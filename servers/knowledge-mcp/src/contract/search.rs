use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use veoveo_embedding_contract::EmbeddingText;
use veoveo_knowledge_contract::{GenerationId, KnowledgeError, MemberTitle};
use veoveo_mcp_knowledge_extension::{CollectionId, EntityKind, Revision};
use veoveo_types::ResourceUri;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "SearchWire", into = "SearchWire")]
pub struct SearchRequest(SearchWire);
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SearchWire {
    query: EmbeddingText,
    #[serde(default)]
    collections: BTreeSet<CollectionId>,
    #[serde(default)]
    entity_kinds: BTreeSet<EntityKind>,
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 20))]
    limit: u16,
}
fn default_limit() -> u16 {
    10
}
impl SearchRequest {
    pub fn new(
        query: EmbeddingText,
        collections: BTreeSet<CollectionId>,
        entity_kinds: BTreeSet<EntityKind>,
        limit: u16,
    ) -> Result<Self, KnowledgeError> {
        SearchWire {
            query,
            collections,
            entity_kinds,
            limit,
        }
        .try_into()
    }
    pub fn query(&self) -> &EmbeddingText {
        &self.0.query
    }
    pub fn collections(&self) -> &BTreeSet<CollectionId> {
        &self.0.collections
    }
    pub fn entity_kinds(&self) -> &BTreeSet<EntityKind> {
        &self.0.entity_kinds
    }
    pub fn limit(&self) -> u16 {
        self.0.limit
    }
}
impl TryFrom<SearchWire> for SearchRequest {
    type Error = KnowledgeError;
    fn try_from(value: SearchWire) -> Result<Self, Self::Error> {
        if !(1..=20).contains(&value.limit)
            || value.collections.len() > 1024
            || value.entity_kinds.len() > 32
        {
            return Err(KnowledgeError(
                "search requires 1..=20 results, at most 1024 collections and 32 kinds",
            ));
        }
        Ok(Self(value))
    }
}
impl From<SearchRequest> for SearchWire {
    fn from(value: SearchRequest) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResultFreshness {
    pub revision: Revision,
    pub modified_at: Option<DateTime<Utc>>,
    pub observed_at: DateTime<Utc>,
    pub stale: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchResult {
    pub uri: ResourceUri,
    pub collection: CollectionId,
    pub title: MemberTitle,
    #[schemars(length(max = 320))]
    pub snippet: String,
    pub score: f64,
    pub freshness: ResultFreshness,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchResponse {
    pub generation: Option<GenerationId>,
    pub results: Vec<SearchResult>,
}
