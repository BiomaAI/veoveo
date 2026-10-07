use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_knowledge_contract::{GenerationId, KnowledgeCollectionApproval};
use veoveo_mcp_knowledge_extension::{CollectionDescriptor, CollectionId};
use veoveo_types::{ResourceUri, ServerSlug};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = dcat_scalar)]
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

fn dcat_scalar(schema: &mut schemars::Schema) {
    use veoveo_types::naming::{
        NamingAuthority, NamingDeclaration, NamingLabel, ScalarNaming, scalar_schema,
    };
    let declaration = NamingDeclaration {
        authority: NamingAuthority::Standard {
            document: veoveo_types::HttpsUrl::parse("https://www.w3.org/TR/vocab-dcat-3/")
                .expect("DCAT standard URL"),
        },
        profile: NamingLabel::new("DCAT class identifiers").expect("DCAT profile"),
        version: NamingLabel::new("3").expect("DCAT revision"),
        applicability: NamingLabel::new("only this DCAT class scalar").expect("DCAT scalar"),
    };
    *schema = scalar_schema(schema.clone(), ScalarNaming::Standard { declaration })
        .expect("DCAT scalar role");
}
