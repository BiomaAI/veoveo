use crate::{
    ServiceError,
    access::SearchCaller,
    contract::{ResultFreshness, SearchRequest, SearchResponse, SearchResult},
    embed::Embeddings,
};
use chrono::Utc;
use std::time::Duration;
use veoveo_embedding_contract::EmbeddingTask;
use veoveo_mcp_knowledge_extension::Freshness;
use veoveo_platform_store::{PlatformStore, knowledge::SearchWindow};

pub struct SearchService<'a, E> {
    pub store: &'a PlatformStore,
    pub embeddings: &'a E,
}
impl<E: Embeddings> SearchService<'_, E> {
    pub async fn search(
        &self,
        caller: &SearchCaller,
        request: &SearchRequest,
    ) -> Result<SearchResponse, ServiceError> {
        tokio::time::timeout(Duration::from_secs(60), self.run(caller, request))
            .await
            .map_err(|_| ServiceError::Deadline)?
    }
    async fn run(
        &self,
        caller: &SearchCaller,
        request: &SearchRequest,
    ) -> Result<SearchResponse, ServiceError> {
        let scope = caller.scope(request.collections());
        let generation = self
            .store
            .active_knowledge_generation(&caller.tenant)
            .await?;
        let Some(generation) = generation else {
            return Ok(SearchResponse {
                generation: None,
                results: vec![],
            });
        };
        if scope.collections.is_empty() {
            return Ok(SearchResponse {
                generation: Some(generation),
                results: vec![],
            });
        }
        let spec = self
            .store
            .knowledge_generation(&caller.tenant, generation)
            .await?
            .ok_or(ServiceError::EmbeddingSpace)?;
        if self.embeddings.space() != spec.space() {
            return Err(ServiceError::EmbeddingSpace);
        }
        let admission = self
            .store
            .admit_knowledge_embeddings(&caller.tenant, generation, self.embeddings.runtime())
            .await?;
        let vector = self
            .embeddings
            .query(
                EmbeddingTask::new(spec.query_task())?,
                request.query().clone(),
            )
            .await?;
        let mut window = SearchWindow::Initial;
        let rows = loop {
            let page = self
                .store
                .search_knowledge(
                    &scope,
                    generation,
                    request.query(),
                    &vector,
                    &admission,
                    request.entity_kinds(),
                    request.limit(),
                    window,
                )
                .await?;
            if page.results.len() >= usize::from(request.limit()) || !page.window_full {
                break page.results;
            }
            window = window.next().ok_or(ServiceError::CandidateBudget)?;
        };
        let now = Utc::now();
        let results = rows
            .into_iter()
            .map(|row| {
                let observation = row.candidate.observation;
                if !caller.allows(&row.descriptor, &row.candidate.uri, &observation, now) {
                    return Err(ServiceError::AccessChanged);
                }
                let stale = match row.descriptor.freshness() {
                    Freshness::Immutable { .. } => false,
                    Freshness::MaxAge { max_age_seconds } => {
                        (now - observation.observed_at()).num_seconds()
                            >= i64::from(max_age_seconds)
                    }
                };
                if stale {
                    return Err(ServiceError::AccessChanged);
                }
                Ok(SearchResult {
                    uri: row.candidate.uri,
                    collection: observation.collection().clone(),
                    title: row.title,
                    snippet: row.candidate.text.chars().take(320).collect(),
                    score: row.score,
                    freshness: ResultFreshness {
                        revision: observation.revision().clone(),
                        modified_at: observation.modified_at(),
                        observed_at: observation.observed_at(),
                        stale,
                    },
                })
            })
            .collect::<Result<_, ServiceError>>()?;
        Ok(SearchResponse {
            generation: Some(generation),
            results,
        })
    }
}
