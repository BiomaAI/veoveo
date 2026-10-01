//! Synthetic embeddings qualify indexing, SQL ranking and policy, never GPU inference.
use chrono::Utc;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
    time::Duration,
};
use veoveo_embedding_contract::*;
use veoveo_knowledge_mcp::{
    ServiceError, access::SearchCaller, contract::*, embed::Embeddings, index::Indexer,
    search::SearchService, source::*,
};
use veoveo_mcp_knowledge_extension::{self as extension, *};
use veoveo_types::*;

#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

struct SyntheticEmbeddings {
    space: EmbeddingSpace,
    inputs: Mutex<Vec<String>>,
}
impl SyntheticEmbeddings {
    fn new() -> Self {
        Self {
            space: EmbeddingSpace {
                model: EmbeddingModelId::new("synthetic-fixture").unwrap(),
                revision: EmbeddingModelRevision::new("fixture-1").unwrap(),
                dimension: EmbeddingDimension::new(3).unwrap(),
                runtime_image: Sha256Digest::from_bytes([1; 32]),
            },
            inputs: Mutex::new(vec![]),
        }
    }
}
impl Embeddings for SyntheticEmbeddings {
    fn space(&self) -> &EmbeddingSpace {
        &self.space
    }
    async fn documents(&self, batch: EmbeddingBatch) -> Result<Vec<EmbeddingVector>, ServiceError> {
        let mut inputs = self.inputs.lock().unwrap();
        Ok(batch
            .texts()
            .iter()
            .map(|t| {
                inputs.push(t.as_str().to_owned());
                EmbeddingVector::new(self.space.clone(), vec![1.0, 0.0, 0.0]).unwrap()
            })
            .collect())
    }
    async fn query(
        &self,
        _: EmbeddingTask,
        _: EmbeddingText,
    ) -> Result<EmbeddingVector, ServiceError> {
        Ok(EmbeddingVector::new(self.space.clone(), vec![1.0, 0.0, 0.0]).unwrap())
    }
}
#[derive(Clone)]
struct Record {
    registration: CollectionRegistration,
    title: MemberTitle,
    text: String,
    access: AccessDescriptor,
    revision: u32,
    fail: bool,
}
struct Source(Mutex<BTreeMap<ResourceUri, Record>>);
impl KnowledgeSource for Source {
    async fn enumerate(&self, uri: ResourceUri) -> Result<SourcePage, ServiceError> {
        let parts = ResourceUriParts::parse(uri.as_str()).unwrap();
        let after = parts
            .query_parameters()
            .get("cursor")
            .map(|s| s.parse::<usize>().unwrap())
            .unwrap_or_default();
        let records = self.0.lock().unwrap();
        let matches: Vec<_> = records
            .iter()
            .filter(|(_, record)| {
                record.registration.descriptor.collection().name().as_str() == parts.authority()
            })
            .collect();
        let items = matches
            .iter()
            .skip(after)
            .take(100)
            .map(|(uri, record)| MemberLink {
                uri: (*uri).clone(),
                title: Some(record.title.clone()),
            })
            .collect();
        Ok(SourcePage::new(
            items,
            (matches.len() > after + 100).then(|| (after + 100).to_string()),
        )?)
    }
    async fn read(&self, uri: ResourceUri) -> Result<SourceDocument, ServiceError> {
        let records = self.0.lock().unwrap();
        let record = records.get(&uri).ok_or(ServiceError::SourceUnavailable)?;
        if record.fail {
            return Err(ServiceError::SourceUnavailable);
        }
        let observation = Observation::builder(
            record.registration.descriptor.collection().clone(),
            Revision::new(record.revision.to_string())?,
            extension::content_digest(&record.text),
            Utc::now(),
        )
        .access(record.access.clone())
        .build(&record.registration.descriptor)?;
        Ok(SourceDocument::new(record.text.clone(), observation)?)
    }
}
fn registration(name: &str, indexing: IndexingMode) -> CollectionRegistration {
    CollectionRegistration {
        tenant: "knowledge-native".parse().unwrap(),
        descriptor: CollectionDescriptor::new(
            CollectionId::new("fixture".parse().unwrap(), name.parse().unwrap()).unwrap(),
            "finding".parse().unwrap(),
            ResourceTemplateUri::new(format!("fixture://{name}{{?cursor}}")).unwrap(),
            Freshness::max_age(300),
            ChangeSignal::Listen,
            AccessModel::WorkContext,
            indexing,
        )
        .unwrap()
        .with_required_scopes([ScopeName::new("fixture:read").unwrap()]),
        approval: CollectionApproval::Index,
        control_revision: Sha256Digest::from_bytes([2; 32]),
    }
}
fn uri(id: &str) -> ResourceUri {
    ResourceUriBuilder::new("fixture://members")
        .unwrap()
        .segment(UriSegment::new(id).unwrap())
        .build()
        .unwrap()
}
fn record(registration: &CollectionRegistration, text: &str) -> Record {
    Record {
        registration: registration.clone(),
        title: MemberTitle::new("Facility inspection").unwrap(),
        text: text.into(),
        revision: 1,
        fail: false,
        access: AccessDescriptor {
            tenant: registration.tenant.clone(),
            work_context: "operations".parse().unwrap(),
            read_policy: ReadPolicy::SelectedWorkContextMembers {},
            owner: AccessSubject::Principal("author".parse().unwrap()),
            grants: vec![],
            data_labels: vec![],
            expires_at: None,
        },
    }
}
fn caller(registrations: &[CollectionRegistration]) -> SearchCaller {
    SearchCaller {
        principal: "reader".parse().unwrap(),
        tenant: registrations[0].tenant.clone(),
        profile: "operations".parse().unwrap(),
        active_work_context: "operations".parse().unwrap(),
        collections: registrations
            .iter()
            .map(|r| r.descriptor.collection().clone())
            .collect(),
        work_contexts: BTreeSet::from(["operations".parse().unwrap()]),
        memberships: BTreeSet::new(),
        scopes: BTreeSet::from(["fixture:read".parse().unwrap()]),
        clearance: BTreeSet::new(),
    }
}

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
        let request = SearchRequest::new(
            EmbeddingText::new("facility").unwrap(),
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
