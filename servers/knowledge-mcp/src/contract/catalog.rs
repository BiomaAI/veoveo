use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_knowledge_contract::{GenerationId, KnowledgeCollectionApproval};
use veoveo_mcp_knowledge_extension::{CollectionDescriptor, CollectionId};
use veoveo_types::{ResourceUri, ServerSlug};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CatalogEntity {
    #[serde(rename = "dcat:DataService")]
    DataService,
    #[serde(rename = "dcat:Dataset")]
    Dataset,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceCatalogEntry {
    #[serde(rename = "type")]
    pub entity: CatalogEntity,
    pub server: ServerSlug,
    pub contract_revision: u32,
    pub uri: ResourceUri,
    pub collections: Vec<CollectionId>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceCatalogPage {
    pub items: Vec<SourceCatalogEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<ServerSlug>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollectionCatalogEntry {
    #[serde(rename = "type")]
    pub entity: CatalogEntity,
    pub uri: ResourceUri,
    pub descriptor: CollectionDescriptor,
    pub approval: KnowledgeCollectionApproval,
    /// Present only when the active generation includes this registration.
    pub generation: Option<GenerationId>,
    /// Counts and timestamps cover only indexed members readable by this caller.
    pub statistics: Option<veoveo_knowledge_contract::CollectionStatistics>,
}
