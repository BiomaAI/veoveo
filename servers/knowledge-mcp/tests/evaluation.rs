//! Synthetic vectors qualify scoring, persistence and corpus checks, not quality.
use std::{collections::BTreeMap, sync::Mutex, time::Duration};
use veoveo_embedding_contract::EmbeddingText;
use veoveo_knowledge_contract::{
    EvaluationCaseId, EvaluationMember, EvaluationMemberId, RetrievalCase, RetrievalDataset,
};
use veoveo_knowledge_mcp::{
    contract::*, embed::Embeddings, evaluation::RetrievalEvaluator, index::Indexer,
    source::MemberLink,
};
use veoveo_mcp_knowledge_extension::{IndexingMode, Revision, content_digest};

#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "support/indexing.rs"]
mod indexing;
use indexing::*;

#[tokio::test]
async fn measured_generation_persists_across_connections_and_rejects_corpus_drift() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let registration = registration("records", IndexingMode::Content);
        let lease =
            db.a.claim_knowledge_coordinator(&registration.tenant, Default::default())
                .await
                .unwrap()
                .unwrap();
        db.a.register_knowledge_collection(&registration, None)
            .await
            .unwrap();
        let embedding = SyntheticEmbeddings::new();
        let spec = GenerationSpec::new(
            embedding.space().clone(),
            "Find relevant passages",
            ChunkSettings::new("structure-v1", 500, 30).unwrap(),
            [(
                registration.descriptor.collection().clone(),
                registration.revision(),
            )]
            .into(),
        )
        .unwrap();
        let mut records = BTreeMap::new();
        let mut corpus = Vec::new();
        for n in 0..20 {
            let address = uri(&n.to_string());
            let text = if n == 0 {
                "needle flood bridge inspection"
            } else {
                "ordinary shelter capacity bulletin"
            };
            records.insert(address.clone(), record(&registration, text));
            corpus.push(EvaluationMember {
                member: EvaluationMemberId {
                    collection: registration.descriptor.collection().clone(),
                    uri: address,
                },
                revision: Revision::new("1").unwrap(),
                content_sha256: content_digest(text),
            });
        }
        let mut denied = record(&registration, "needle private report");
        denied.access.data_labels.push("secret".parse().unwrap());
        records.insert(uri("denied"), denied);
        let source = Source(Mutex::new(records));
        let indexer = Indexer {
            store: &db.a,
            lease: &lease,
            source: &source,
            embeddings: &embedding,
        };
        let generation = indexer
            .build(
                &registration.tenant,
                std::slice::from_ref(&registration),
                &spec,
            )
            .await
            .unwrap();
        db.a.activate_knowledge_generation(&lease, &registration.tenant, generation, None)
            .await
            .unwrap();
        let dataset = RetrievalDataset::new(
            corpus.clone(),
            vec![
                RetrievalCase::new(
                    EvaluationCaseId::new("bridge").unwrap(),
                    EmbeddingText::new("needle").unwrap(),
                    Default::default(),
                    [corpus[0].member.clone()].into(),
                )
                .unwrap(),
            ],
        )
        .unwrap();
        let reader = caller(std::slice::from_ref(&registration));
        let evaluator = RetrievalEvaluator {
            store: &db.b,
            embeddings: &embedding,
        };
        let result = evaluator.evaluate(&reader, &dataset).await.unwrap();
        assert_eq!(result.recall_at_ten(), 1.0);
        assert_eq!(result.measurements()[0].ranked.len(), 10);
        let revision =
            db.a.record_knowledge_evaluation(&registration.tenant, &result)
                .await
                .unwrap();
        assert_eq!(
            db.b.record_knowledge_evaluation(&registration.tenant, &result)
                .await
                .unwrap(),
            revision
        );
        let loaded =
            db.b.knowledge_evaluation(&registration.tenant, generation, &revision)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(loaded.revision(), result.revision());
        assert_eq!(loaded.recall_at_ten(), 1.0);
        assert!(
            db.b.knowledge_evaluation(&"other-tenant".parse().unwrap(), generation, &revision)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            db.b.record_knowledge_evaluation(&"other-tenant".parse().unwrap(), &result)
                .await
                .is_err()
        );
        let mut wrong = serde_json::to_value(&result).unwrap();
        wrong["specificationRevision"] =
            serde_json::to_value(veoveo_types::Sha256Digest::from_bytes([9; 32])).unwrap();
        assert!(
            db.b.record_knowledge_evaluation(
                &registration.tenant,
                &serde_json::from_value(wrong).unwrap()
            )
            .await
            .is_err()
        );
        let omitted =
            RetrievalDataset::new(corpus[..19].to_vec(), dataset.cases().to_vec()).unwrap();
        assert!(
            evaluator.evaluate(&reader, &omitted).await.is_err(),
            "unjudged indexed members cannot silently change the comparison corpus"
        );
        source
            .0
            .lock()
            .unwrap()
            .get_mut(&uri("0"))
            .unwrap()
            .revision = 2;
        indexer
            .refresh(
                &registration,
                generation,
                &spec,
                &MemberLink {
                    uri: uri("0"),
                    title: None,
                },
            )
            .await
            .unwrap();
        assert!(
            evaluator.evaluate(&reader, &dataset).await.is_err(),
            "old judgments require their recorded source revisions"
        );
        assert!(
            db.b.knowledge_evaluation(&registration.tenant, generation, &revision)
                .await
                .unwrap()
                .is_some(),
            "historical observations survive source updates"
        );
        // Retiring and reclaiming a generation owns its measurement cleanup.
        let replacement = indexer
            .build(
                &registration.tenant,
                std::slice::from_ref(&registration),
                &spec,
            )
            .await
            .unwrap();
        db.a.activate_knowledge_generation(
            &lease,
            &registration.tenant,
            replacement,
            Some(generation),
        )
        .await
        .unwrap();
        db.a.remove_knowledge_generation(&lease, &registration.tenant, generation)
            .await
            .unwrap();
        assert!(
            db.b.knowledge_evaluation(&registration.tenant, generation, &revision)
                .await
                .unwrap()
                .is_none()
        );
    })
    .await
    .expect("retrieval evaluation qualification exceeded 180 seconds");
}
