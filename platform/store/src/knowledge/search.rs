use super::{reads::Row, *};
use crate::PlatformStore;
use std::collections::BTreeSet;
use veoveo_embedding_contract::{EmbeddingText, EmbeddingVector};
use veoveo_mcp_knowledge_extension::{CollectionDescriptor, EntityKind};

/// Closed expansion budgets also supply the numeric HNSW query syntax.
#[derive(Debug, Clone, Copy)]
pub enum SearchWindow {
    Initial,
    Expanded,
    Deep,
    Maximum,
}
impl SearchWindow {
    pub fn candidates(self) -> u16 {
        match self {
            Self::Initial => 128,
            Self::Expanded => 512,
            Self::Deep => 2048,
            Self::Maximum => 8192,
        }
    }
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Initial => Some(Self::Expanded),
            Self::Expanded => Some(Self::Deep),
            Self::Deep => Some(Self::Maximum),
            Self::Maximum => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RankedCandidate {
    pub candidate: KnowledgeCandidate,
    pub descriptor: CollectionDescriptor,
    pub score: f64,
    pub title: veoveo_knowledge_contract::MemberTitle,
}
#[derive(Debug, Clone)]
pub struct HybridSearchPage {
    pub results: Vec<RankedCandidate>,
    /// A full SQL ranking window may need expansion after grouping chunks.
    /// Short ANN pages are completed by exact SQL distance ranking first.
    pub window_full: bool,
}
#[derive(SurrealValue)]
struct RankedRow {
    title: String,
    tenant: String,
    collection_id: String,
    uri: String,
    ordinal: i64,
    text: String,
    admission: admission::Admission,
    observation: Document<veoveo_mcp_knowledge_extension::Observation>,
    descriptor: Document<CollectionDescriptor>,
    rrf_score: f64,
}
#[derive(SurrealValue)]
struct Page {
    rows: Vec<RankedRow>,
    lexical_count: i64,
    semantic_count: i64,
}

impl PlatformStore {
    #[allow(
        clippy::too_many_arguments,
        reason = "typed retrieval inputs and one explicit window"
    )]
    pub async fn search_knowledge(
        &self,
        scope: &CandidateScope,
        generation: GenerationId,
        query: &EmbeddingText,
        vector: &EmbeddingVector,
        entity_kinds: &BTreeSet<EntityKind>,
        limit: u16,
        window: SearchWindow,
    ) -> Result<HybridSearchPage, StoreError> {
        scope.validate()?;
        if !(1..=20).contains(&limit) || entity_kinds.len() > 32 {
            return Err(StoreError::Knowledge(
                "search exceeds its result or kind bound",
            ));
        }
        let spec = self
            .knowledge_generation(&scope.tenant, generation)
            .await?
            .ok_or(StoreError::Knowledge("search generation is missing"))?;
        if vector.space() != spec.space() {
            return Err(StoreError::Knowledge(
                "query vector belongs to another embedding space",
            ));
        }
        let count = window.candidates();
        let sql = include_str!("search.surql")
            .replace("__ADMISSION__", include_str!("admitted.surql"))
            .replace(
                "__URI_SELECTION__",
                include_str!("resource_selection.surql"),
            )
            .replace("__TABLE__", &chunk_table(generation))
            .replace("__CANDIDATES__", &count.to_string())
            .replace("__EF__", &(u32::from(count) * 2).to_string());
        let mut response = scope
            .bind(self.client().query(sql), generation)
            .bind(("query", query.as_str().to_owned()))
            .bind(("vector", vector.values().to_vec()))
            .bind((
                "entity_kinds",
                entity_kinds
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind(("limit", i64::from(limit)))
            .bind(("candidates", i64::from(count)))
            .await?
            .knowledge_check()?;
        let index = response
            .num_statements()
            .checked_sub(2)
            .ok_or(StoreError::Knowledge("search response missing"))?;
        let page: Option<Page> = response.take(index)?;
        let page = page.ok_or(StoreError::Knowledge("search response missing"))?;
        let results = page
            .rows
            .into_iter()
            .map(|row| {
                let candidate = Row {
                    tenant: row.tenant,
                    collection_id: row.collection_id,
                    uri: row.uri,
                    ordinal: row.ordinal,
                    text: row.text,
                    admission: row.admission,
                    observation: row.observation,
                }
                .checked(scope, generation)?;
                let descriptor = row.descriptor.0;
                if !row.rrf_score.is_finite()
                    || row.rrf_score <= 0.0
                    || candidate
                        .observation
                        .validate_collection(&descriptor)
                        .is_err()
                {
                    return integrity();
                }
                let title = veoveo_knowledge_contract::MemberTitle::new(row.title)
                    .map_err(|_| StoreError::Knowledge("invalid stored member title"))?;
                Ok(RankedCandidate {
                    candidate,
                    descriptor,
                    score: row.rrf_score,
                    title,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        Ok(HybridSearchPage {
            results,
            window_full: page.lexical_count >= i64::from(count)
                || page.semantic_count >= i64::from(count),
        })
    }
}
