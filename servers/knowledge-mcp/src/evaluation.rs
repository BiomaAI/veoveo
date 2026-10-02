//! Retrieval evaluation through the production SQL admission and ranking path.
use crate::{
    ServiceError, access::SearchCaller, contract::SearchRequest, embed::Embeddings,
    search::SearchService,
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use veoveo_knowledge_contract::{
    EvaluationMember, EvaluationMemberId, GenerationId, KnowledgeError, RetrievalDataset,
    RetrievalEvaluation, RetrievalMeasurement,
};
use veoveo_platform_store::PlatformStore;
use veoveo_types::Sha256Digest;

pub struct RetrievalEvaluator<'a, E> {
    pub store: &'a PlatformStore,
    pub embeddings: &'a E,
}
impl<E: Embeddings> RetrievalEvaluator<'_, E> {
    /// Require the complete selected, caller-visible corpus to match the judged
    /// manifest before and after querying. The source owners retain authorization.
    /// Evaluation records observations during this interval, not future quality.
    pub async fn evaluate(
        &self,
        caller: &SearchCaller,
        dataset: &RetrievalDataset,
    ) -> Result<RetrievalEvaluation, ServiceError> {
        tokio::time::timeout(Duration::from_secs(3600), self.run(caller, dataset))
            .await
            .map_err(|_| ServiceError::Deadline)?
    }

    async fn run(
        &self,
        caller: &SearchCaller,
        dataset: &RetrievalDataset,
    ) -> Result<RetrievalEvaluation, ServiceError> {
        let started_at = Utc::now();
        let generation = self
            .store
            .active_knowledge_generation(&caller.tenant)
            .await?
            .ok_or(KnowledgeError(
                "retrieval evaluation requires an active generation",
            ))?;
        let spec = self
            .store
            .knowledge_generation(&caller.tenant, generation)
            .await?
            .ok_or(ServiceError::EmbeddingSpace)?;
        if self.embeddings.space() != spec.space() {
            return Err(ServiceError::EmbeddingSpace);
        }
        let collections = dataset.collections();
        if collections
            .iter()
            .any(|id| !spec.collections().contains_key(id) || !caller.collections.contains_key(id))
        {
            return Err(KnowledgeError(
                "evaluation corpus is outside the generation or caller's collections",
            )
            .into());
        }
        let corpus: BTreeMap<_, _> = dataset
            .corpus()
            .iter()
            .map(|m| (m.member.clone(), m.clone()))
            .collect();
        self.check_corpus(caller, generation, dataset, &corpus)
            .await?;
        let mut evaluation = RetrievalEvaluation::builder(
            generation,
            spec.revision(),
            audience_revision(caller, dataset),
            dataset.clone(),
            started_at,
        );
        let search = SearchService {
            store: self.store,
            embeddings: self.embeddings,
        };
        for case in dataset.cases() {
            let selected = if case.collections().is_empty() {
                collections.clone()
            } else {
                case.collections().clone()
            };
            let request =
                SearchRequest::new(case.query().clone(), selected, Default::default(), 10)?;
            let began = Instant::now();
            let response = search.search(caller, &request).await?;
            let elapsed_micros = began.elapsed().as_micros().max(1) as u64;
            if response.generation != Some(generation) {
                return Err(
                    KnowledgeError("generation changed during retrieval evaluation").into(),
                );
            }
            let mut ranked = Vec::new();
            for result in response.results {
                let member = EvaluationMemberId {
                    collection: result.collection,
                    uri: result.uri,
                };
                if !corpus
                    .get(&member)
                    .is_some_and(|expected| expected.revision == result.freshness.revision)
                    || result.freshness.stale
                {
                    return Err(KnowledgeError(
                        "retrieval result does not match the evaluated source revision",
                    )
                    .into());
                }
                ranked.push(member);
            }
            evaluation = evaluation.measure(RetrievalMeasurement {
                case: case.id().clone(),
                ranked,
                elapsed_micros,
            });
        }
        self.check_corpus(caller, generation, dataset, &corpus)
            .await?;
        Ok(evaluation.finish(Utc::now())?)
    }

    async fn check_corpus(
        &self,
        caller: &SearchCaller,
        generation: GenerationId,
        dataset: &RetrievalDataset,
        expected: &BTreeMap<EvaluationMemberId, EvaluationMember>,
    ) -> Result<(), ServiceError> {
        let scope = caller.scope(&dataset.collections());
        let mut cursor = None;
        let mut found = BTreeMap::new();
        // The dataset caps members at 4096 and each member at 256 chunks.
        for _ in 0..10_487 {
            let page = self
                .store
                .knowledge_candidates_page(&scope, generation, cursor.as_ref(), 100)
                .await?;
            for candidate in &page {
                let member = EvaluationMemberId {
                    collection: candidate.observation.collection().clone(),
                    uri: candidate.uri.clone(),
                };
                let observed = EvaluationMember {
                    member: member.clone(),
                    revision: candidate.observation.revision().clone(),
                    content_sha256: candidate.observation.content_sha256().clone(),
                };
                if expected.get(&member) != Some(&observed) {
                    return Err(KnowledgeError(
                        "caller-visible index differs from the evaluation corpus",
                    )
                    .into());
                }
                found.insert(member, observed);
            }
            if page.len() < 100 {
                if &found != expected
                    || self
                        .store
                        .active_knowledge_generation(&caller.tenant)
                        .await?
                        != Some(generation)
                {
                    return Err(KnowledgeError(
                        "evaluation corpus is incomplete or its generation changed",
                    )
                    .into());
                }
                return Ok(());
            }
            cursor = page.last().map(|row| row.cursor());
        }
        Err(ServiceError::Traversal)
    }
}

fn audience_revision(caller: &SearchCaller, dataset: &RetrievalDataset) -> Sha256Digest {
    let scope = caller.scope(&dataset.collections());
    let bytes = serde_json::to_vec(&(
        &caller.principal,
        &scope.tenant,
        &scope.profile,
        &scope.active_work_context,
        &scope.collections,
        &scope.work_contexts,
        &scope.subjects,
        &scope.scopes,
        &scope.clearance,
    ))
    .expect("typed evaluation audience");
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
