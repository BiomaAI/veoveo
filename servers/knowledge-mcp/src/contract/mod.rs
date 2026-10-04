mod catalog;
mod embed;
pub use catalog::{CatalogEntity, CollectionCatalogEntry, SourceCatalogEntry, SourceCatalogPage};
pub use embed::{EmbedRequest, EmbedResponse};
mod resource;
mod search;
pub use resource::KnowledgeResource;
pub use search::{ResultFreshness, SearchRequest, SearchResponse, SearchResult};
pub use veoveo_knowledge_contract::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
pub enum KnowledgeScope {
    #[vocabulary(rename = "knowledge:read")]
    Read,
    #[vocabulary(rename = "knowledge:search")]
    Search,
    #[vocabulary(rename = "knowledge:embed")]
    Embed,
    #[vocabulary(rename = "knowledge:admin")]
    Admin,
}
