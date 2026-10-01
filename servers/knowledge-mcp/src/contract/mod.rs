mod resource;
mod search;
pub use resource::KnowledgeResource;
pub use search::{ResultFreshness, SearchRequest, SearchResponse, SearchResult};
pub use veoveo_knowledge_contract::*;

veoveo_types::scope_enum! {
    pub enum KnowledgeScope {
        Read => "knowledge:read",
        Search => "knowledge:search",
        Embed => "knowledge:embed",
        Admin => "knowledge:admin",
    }
}
