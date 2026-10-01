//! Synthetic embeddings qualify indexing, SQL ranking and policy, never GPU inference.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
    time::Duration,
};
use veoveo_embedding_contract::*;
use veoveo_knowledge_mcp::{
    contract::*, index::Indexer, search::SearchService, source::MemberLink,
};
use veoveo_mcp_knowledge_extension::*;
use veoveo_types::*;

#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

#[path = "support/indexing.rs"]
mod indexing;
use indexing::*;

#[tokio::test]
async fn source_to_hybrid_search_with_sql_policy_metadata_and_invalidation() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let content = registration("records", IndexingMode::Content);
        let metadata = registration("metadata", IndexingMode::Metadata);
        let registrations = vec![content.clone(), metadata.clone()];
        for registration in &registrations { db.a.register_knowledge_collection(registration, None).await.unwrap(); }
        let embedding = SyntheticEmbeddings::new();
        let spec = GenerationSpec::new(embedding.space.clone(), "Find relevant passages", ChunkSettings::new("structure-v1", 500, 30).unwrap(), registrations.iter().map(|r| (r.descriptor.collection().clone(), r.revision())).collect()).unwrap();
        let mut records = BTreeMap::new();
        for n in 0..140 {
            let mut hidden = record(&content, "facility secret inspection");
            hidden.access.data_labels.push("secret".parse().unwrap());
            hidden.title = MemberTitle::new("Hidden title").unwrap();
            records.insert(uri(&format!("aaa-hidden-{n:03}")), hidden);
        }
        records.insert(uri("public-a"), record(&content, "facility inspection identifies access routes"));
        records.insert(uri("public-b"), record(&content, "facility inspection identifies safety issues"));
        records.insert(uri("metadata"), record(&metadata, "BODY-MUST-NEVER-BE-EMBEDDED"));
        let source = Source(Mutex::new(records));
        let indexer = Indexer { store: &db.a, source: &source, embeddings: &embedding };
        let generation = indexer.build(&content.tenant, &registrations, &spec).await.unwrap();
        assert!(!embedding.inputs.lock().unwrap().iter().any(|text| text.contains("BODY-MUST-NEVER")));
        db.a.activate_knowledge_generation(&content.tenant, generation, None).await.unwrap();

        // A denied row that cannot be decoded proves SQL admits before decoding.
        let table = format!("knowledge_chunk_{}", generation.as_uuid().simple());
        db.a.client().query(format!("UPDATE {table} SET observation = {{malformed: true}} WHERE admission.labels CONTAINS 'secret';")).await.unwrap().check().unwrap();
        let mut reader = caller(&registrations);
        let request = SearchRequest::new(EmbeddingText::new("facility").unwrap(), BTreeSet::new(), BTreeSet::new(), 3).unwrap();
        let search = SearchService { store: &db.b, embeddings: &embedding };
        let result = search.search(&reader, &request).await.unwrap();
        assert_eq!(result.results.len(), 3);
        assert!(result.results.iter().any(|r| r.score > 0.025), "native RRF combines lexical and semantic ranks");
        assert!(result.results.iter().all(|r| !r.uri.as_str().contains("hidden") && r.title.as_str() != "Hidden title" && r.score > 0.0 && !r.freshness.stale));
        let semantic_only = SearchRequest::new(EmbeddingText::new("unmatchedkeyword").unwrap(), BTreeSet::new(), BTreeSet::new(), 3).unwrap();
        assert_eq!(search.search(&reader, &semantic_only).await.unwrap().results.len(), 3, "HNSW filters denied nearest neighbours before K even without lexical matches");
        reader.scopes.clear();
        assert!(search.search(&reader, &request).await.unwrap().results.is_empty());
        reader = caller(&registrations);
        reader.active_work_context = "elsewhere".parse().unwrap();
        assert!(search.search(&reader, &request).await.unwrap().results.is_empty());
        reader = caller(&registrations);

        // A failed refresh hides cached chunks immediately, then a later valid
        // revision can settle a fresh read ticket without restoring denied rows.
        source.0.lock().unwrap().get_mut(&uri("public-a")).unwrap().fail = true;
        let link = MemberLink { uri: uri("public-a"), title: None };
        assert!(indexer.refresh(&content, generation, &spec, &link).await.is_err());
        assert_eq!(search.search(&reader, &request).await.unwrap().results.len(), 2);
        {
            let mut records = source.0.lock().unwrap();
            let updated = records.get_mut(&link.uri).unwrap(); updated.fail = false; updated.revision = 2;
            updated.access.data_labels.push("secret".parse().unwrap());
        }
        indexer.refresh(&content, generation, &spec, &link).await.unwrap();
        assert_eq!(search.search(&reader, &request).await.unwrap().results.len(), 2);
        let kinds = SearchRequest::new(EmbeddingText::new("facility").unwrap(), BTreeSet::new(), BTreeSet::from(["different-kind".parse().unwrap()]), 3).unwrap();
        assert!(search.search(&reader, &kinds).await.unwrap().results.is_empty());
    }).await.expect("knowledge pipeline exceeded 180 seconds");
}

#[tokio::test]
async fn duplicate_chunks_expand_the_window_and_failed_rebuild_preserves_active() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let registration = registration("records", IndexingMode::Content);
        db.a.register_knowledge_collection(&registration, None)
            .await
            .unwrap();
        let embedding = SyntheticEmbeddings::new();
        let spec = GenerationSpec::new(
            embedding.space.clone(),
            "Find passages",
            ChunkSettings::new("structure-v1", 500, 0).unwrap(),
            BTreeMap::from([(
                registration.descriptor.collection().clone(),
                registration.revision(),
            )]),
        )
        .unwrap();
        let many = (0..200)
            .map(|n| format!("# Section {n}\nfacility inspection\n"))
            .collect::<String>();
        let source = Source(Mutex::new(BTreeMap::from([
            (uri("a-many"), record(&registration, &many)),
            (uri("b-one"), record(&registration, "facility entrance")),
            (uri("c-one"), record(&registration, "facility exit")),
        ])));
        let indexer = Indexer {
            store: &db.a,
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
        db.a.activate_knowledge_generation(&registration.tenant, generation, None)
            .await
            .unwrap();
        for query in ["facility", "unmatchedkeyword"] {
            let request = SearchRequest::new(
                EmbeddingText::new(query).unwrap(),
                BTreeSet::new(),
                BTreeSet::new(),
                3,
            )
            .unwrap();
            let results = SearchService {
                store: &db.b,
                embeddings: &embedding,
            }
            .search(&caller(std::slice::from_ref(&registration)), &request)
            .await
            .unwrap();
            assert_eq!(results.results.len(), 3);
            assert_eq!(
                results
                    .results
                    .iter()
                    .map(|r| &r.uri)
                    .collect::<BTreeSet<_>>()
                    .len(),
                3
            );
        }
        source
            .0
            .lock()
            .unwrap()
            .get_mut(&uri("a-many"))
            .unwrap()
            .fail = true;
        assert!(
            indexer
                .build(
                    &registration.tenant,
                    std::slice::from_ref(&registration),
                    &spec
                )
                .await
                .is_err()
        );
        assert_eq!(
            db.b.active_knowledge_generation(&registration.tenant)
                .await
                .unwrap(),
            Some(generation)
        );
    })
    .await
    .expect("knowledge window test exceeded 180 seconds");
}

#[tokio::test]
async fn profile_uri_selection_precedes_both_rankings_and_denied_row_decoding() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let registration = registration("records", IndexingMode::Content);
        db.a.register_knowledge_collection(&registration, None).await.unwrap();
        let embedding = SyntheticEmbeddings::new();
        let spec = GenerationSpec::new(embedding.space.clone(), "Find passages", ChunkSettings::new("structure-v1", 500, 0).unwrap(),
            BTreeMap::from([(registration.descriptor.collection().clone(), registration.revision())])).unwrap();
        let mut records = BTreeMap::new();
        for n in 0..145 {
            records.insert(uri(&format!("aaa-denied-{n:03}")), record(&registration, "facility inspection"));
        }
        for id in ["public-a-end", "public-b-end", "public-c-end", "public-x-end-end"] {
            records.insert(uri(id), record(&registration, "facility inspection"));
        }
        let source = Source(Mutex::new(records));
        let generation = Indexer { store: &db.a, source: &source, embeddings: &embedding }
            .build(&registration.tenant, std::slice::from_ref(&registration), &spec).await.unwrap();
        db.a.activate_knowledge_generation(&registration.tenant, generation, None).await.unwrap();
        let table = format!("knowledge_chunk_{}", generation.as_uuid().simple());
        db.a.client().query(format!("UPDATE {table} SET observation = {{malformed: true}} WHERE string::contains(uri, 'aaa-denied') OR string::ends_with(uri, 'x-end-end');"))
            .await.unwrap().check().unwrap();
        let mut reader = caller(std::slice::from_ref(&registration));
        reader.collections.get_mut(registration.descriptor.collection()).unwrap().selectors = vec![ResourceSelector::Template {
            uri_template: ResourceUriTemplate::new("fixture://members/public-{id}-end").unwrap(),
        }];
        let search = SearchService { store: &db.b, embeddings: &embedding };
        // The suffix repeated twice must fail the deterministic template match.
        // A regex with backtracking would select its malformed observation.
        for query in ["facility", "unmatchedkeyword"] {
            let request = SearchRequest::new(EmbeddingText::new(query).unwrap(), BTreeSet::new(), BTreeSet::new(), 3).unwrap();
            let results = search.search(&reader, &request).await.unwrap().results;
            assert_eq!(results.len(), 3, "{query}: selectors must run before ranking LIMIT");
            assert_eq!(results.into_iter().map(|result| result.uri).collect::<BTreeSet<_>>(),
                [uri("public-a-end"), uri("public-b-end"), uri("public-c-end")].into_iter().collect());
        }
        // Revoking profile exposure takes effect without rewriting the index.
        reader.collections.get_mut(registration.descriptor.collection()).unwrap().selectors.clear();
        let request = SearchRequest::new(EmbeddingText::new("facility").unwrap(), BTreeSet::new(), BTreeSet::new(), 3).unwrap();
        assert!(search.search(&reader, &request).await.unwrap().results.is_empty());
    }).await.expect("profile URI retrieval exceeded 180 seconds");
}

#[tokio::test]
async fn installation_label_ceiling_stops_source_text_before_embedding() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let registration = registration("records", IndexingMode::Content);
        db.a.register_knowledge_collection(&registration, None)
            .await
            .unwrap();
        let embedding = SyntheticEmbeddings::new();
        let spec = GenerationSpec::new(
            embedding.space.clone(),
            "Find passages",
            ChunkSettings::new("structure-v1", 500, 0).unwrap(),
            [(
                registration.descriptor.collection().clone(),
                registration.revision(),
            )]
            .into(),
        )
        .unwrap();
        let mut restricted = record(&registration, "CONTENT-OUTSIDE-INSTALLATION-APPROVAL");
        restricted
            .access
            .data_labels
            .push("unapproved".parse().unwrap());
        let source = Source(Mutex::new([(uri("restricted"), restricted)].into()));
        assert!(
            Indexer {
                store: &db.a,
                source: &source,
                embeddings: &embedding
            }
            .build(
                &registration.tenant,
                std::slice::from_ref(&registration),
                &spec
            )
            .await
            .is_err()
        );
        assert!(
            embedding.inputs.lock().unwrap().is_empty(),
            "no outside-approval content may reach embeddings"
        );
        assert!(
            db.a.active_knowledge_generation(&registration.tenant)
                .await
                .unwrap()
                .is_none()
        );
    })
    .await
    .expect("installation approval qualification exceeded 180 seconds");
}
