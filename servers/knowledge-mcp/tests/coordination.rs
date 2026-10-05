//! Synthetic embeddings qualify source coordination, not inference or GPU quality.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "support/indexing.rs"]
mod indexing;
use indexing::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, Semaphore, broadcast, watch};
use tokio_util::sync::CancellationToken;
use veoveo_embedding_contract::EmbeddingText;
use veoveo_knowledge_mcp::{
    ServiceError,
    contract::*,
    coordinator::{Coordinator, CoordinatorState},
    search::SearchService,
    source::*,
};
use veoveo_mcp_knowledge_extension::{
    ChangeSignal, CollectionDescriptor, Freshness, IndexingMode, Observation,
};
use veoveo_types::ResourceUri;

struct ObservedSource {
    inner: Source,
    events: broadcast::Sender<bool>,
    listening: AtomicUsize,
    pause_once: AtomicBool,
    reading: Notify,
    hold: Semaphore,
}
struct Listener(broadcast::Receiver<bool>);
impl SourceListener for Listener {
    async fn changed(&mut self) -> Result<(), ServiceError> {
        match self.0.recv().await {
            Ok(false) => Ok(()),
            _ => Err(ServiceError::SourceUnavailable),
        }
    }
}
impl ObservableSource for ObservedSource {
    type Listener = Listener;
    async fn listen(&self, _: &CollectionDescriptor) -> Result<Listener, ServiceError> {
        self.listening.fetch_add(1, Ordering::SeqCst);
        Ok(Listener(self.events.subscribe()))
    }
}
impl KnowledgeSource for ObservedSource {
    async fn enumerate(
        &self,
        collection: &CollectionDescriptor,
        uri: ResourceUri,
    ) -> Result<SourcePage, ServiceError> {
        if collection.change_signal() == ChangeSignal::Listen {
            assert!(
                self.listening.load(Ordering::SeqCst) > 0,
                "listen must acknowledge before enumeration"
            );
        }
        self.inner.enumerate(collection, uri).await
    }
    async fn read(
        &self,
        collection: &CollectionDescriptor,
        uri: ResourceUri,
        previous: Option<&Observation>,
    ) -> Result<SourceRead, ServiceError> {
        let result = self.inner.read(collection, uri, previous).await?;
        if self.pause_once.swap(false, Ordering::SeqCst) {
            self.reading.notify_one();
            let _permit = self.hold.acquire().await.unwrap();
        }
        Ok(result)
    }
}
async fn ready(status: &mut watch::Receiver<CoordinatorState>) -> GenerationId {
    loop {
        status.changed().await.unwrap();
        match *status.borrow_and_update() {
            CoordinatorState::Ready(id) => return id,
            CoordinatorState::Failed | CoordinatorState::Stopped => {
                panic!("coordinator stopped before ready")
            }
            _ => (),
        }
    }
}
fn specification(
    registration: &CollectionRegistration,
    embedding: &SyntheticEmbeddings,
) -> GenerationSpec {
    GenerationSpec::new(
        embedding.space.clone(),
        "Find passages",
        ChunkSettings::new("structure-v1", 500, 0).unwrap(),
        BTreeMap::from([(
            registration.descriptor.collection().clone(),
            registration.revision(),
        )]),
    )
    .unwrap()
}

#[tokio::test]
async fn revalidation_profile_refreshes_without_listeners_or_new_embeddings() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let mut registration = registration("records", IndexingMode::Content);
        let descriptor = &registration.descriptor;
        registration.descriptor = CollectionDescriptor::new(
            descriptor.collection().clone(),
            descriptor.entity_kind().clone(),
            descriptor.enumerate().clone(),
            Freshness::max_age(2),
            ChangeSignal::Revalidate,
            descriptor.access(),
            descriptor.indexing(),
        )
        .unwrap()
        .with_required_scopes(descriptor.required_scopes().iter().cloned());
        db.a.register_knowledge_collection(&registration, None)
            .await
            .unwrap();
        let embedding = SyntheticEmbeddings::new();
        let spec = specification(&registration, &embedding);
        let source = ObservedSource {
            inner: Source(Mutex::new(
                [(uri("a"), record(&registration, "facility review"))].into(),
            )),
            events: broadcast::channel(32).0,
            listening: AtomicUsize::new(0),
            pause_once: AtomicBool::new(false),
            reading: Notify::new(),
            hold: Semaphore::new(0),
        };
        let coordinator = Coordinator {
            store: &db.a,
            source: &source,
            embeddings: &embedding,
        };
        let cancel = CancellationToken::new();
        let (sender, mut status) = watch::channel(CoordinatorState::Starting);
        let readiness =
            veoveo_knowledge_mcp::indexing::IndexingReadiness::new(vec![status.clone()]).unwrap();
        let (outcome, _) = tokio::join!(
            coordinator.run(
                &registration.tenant,
                std::slice::from_ref(&registration),
                &spec,
                cancel.clone(),
                sender
            ),
            async {
                let generation = ready(&mut status).await;
                assert!(readiness.is_ready());
                let search = SearchService {
                    store: &db.b,
                    embeddings: &embedding,
                };
                let reader = caller(std::slice::from_ref(&registration));
                let request = SearchRequest::new(
                    EmbeddingText::new("facility").unwrap(),
                    BTreeSet::new(),
                    BTreeSet::new(),
                    3,
                )
                .unwrap();
                let before = search
                    .search(&reader, &request)
                    .await
                    .unwrap()
                    .results
                    .remove(0);
                source.pause_once.store(true, Ordering::SeqCst);
                source.reading.notified().await;
                assert_eq!(*status.borrow(), CoordinatorState::Updating(generation));
                assert!(
                    readiness.is_ready(),
                    "reconciliation must keep the HTTP endpoint ready"
                );
                source.hold.add_permits(1);
                assert_eq!(ready(&mut status).await, generation);
                let after = search
                    .search(&reader, &request)
                    .await
                    .unwrap()
                    .results
                    .remove(0);
                assert!(after.freshness.observed_at > before.freshness.observed_at);
                assert_eq!(after.freshness.revision, before.freshness.revision);
                assert_eq!(embedding.inputs.lock().unwrap().len(), 1);
                assert_eq!(source.listening.load(Ordering::SeqCst), 0);
                cancel.cancel();
            }
        );
        outcome.unwrap();
    })
    .await
    .expect("scheduled revalidation qualification exceeded 180 seconds");
}

#[tokio::test]
async fn listener_precedes_reads_and_changes_during_build_settle_before_activation() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let registration = registration("records", IndexingMode::Content);
        db.a.register_knowledge_collection(&registration, None)
            .await
            .unwrap();
        let embedding = SyntheticEmbeddings::new();
        let spec = specification(&registration, &embedding);
        let source = ObservedSource {
            inner: Source(Mutex::new(
                [(
                    uri("a"),
                    record(&registration, "obsolete facility analysis"),
                )]
                .into(),
            )),
            events: broadcast::channel(32).0,
            listening: AtomicUsize::new(0),
            pause_once: AtomicBool::new(true),
            reading: Notify::new(),
            hold: Semaphore::new(0),
        };
        let coordinator = Coordinator {
            store: &db.a,
            source: &source,
            embeddings: &embedding,
        };
        let cancel = CancellationToken::new();
        let (sender, mut status) = watch::channel(CoordinatorState::Starting);
        let (outcome, _) = tokio::join!(
            coordinator.run(
                &registration.tenant,
                std::slice::from_ref(&registration),
                &spec,
                cancel.clone(),
                sender
            ),
            async {
                source.reading.notified().await;
                assert!(
                    db.b.active_knowledge_generation(&registration.tenant)
                        .await
                        .unwrap()
                        .is_none()
                );
                let mut before =
                    db.b.client()
                        .query(include_str!("queries/coordination/listener_precedes_reads_and_changes_during_build_settle_before_activation.surql"))
                        .await
                        .unwrap()
                        .check()
                        .unwrap();
                let before: Vec<chrono::DateTime<chrono::Utc>> = before.take(0).unwrap();
                // A slow source read must not stop the ten-second lease renewal.
                tokio::time::sleep(Duration::from_secs(11)).await;
                let mut after =
                    db.b.client()
                        .query(include_str!("queries/coordination/listener_precedes_reads_and_changes_during_build_settle_before_activation_2.surql"))
                        .await
                        .unwrap()
                        .check()
                        .unwrap();
                let after: Vec<chrono::DateTime<chrono::Utc>> = after.take(0).unwrap();
                assert!(after[0] > before[0]);
                {
                    let mut records = source.inner.0.lock().unwrap();
                    let record = records.get_mut(&uri("a")).unwrap();
                    record.text = "current facility analysis".into();
                    record.revision = 2;
                }
                source.events.send(false).unwrap();
                let generation = ready(&mut status).await;
                assert_eq!(
                    db.b.active_knowledge_generation(&registration.tenant)
                        .await
                        .unwrap(),
                    Some(generation)
                );
                assert_eq!(
                    *embedding.inputs.lock().unwrap(),
                    ["current facility analysis"]
                );
                cancel.cancel();
            }
        );
        outcome.unwrap();
        assert_eq!(*status.borrow(), CoordinatorState::Stopped);
        let search = SearchService {
            store: &db.b,
            embeddings: &embedding,
        };
        let request = SearchRequest::new(
            EmbeddingText::new("facility").unwrap(),
            BTreeSet::new(),
            BTreeSet::new(),
            3,
        )
        .unwrap();
        assert!(
            search
                .search(&caller(std::slice::from_ref(&registration)), &request)
                .await
                .unwrap()
                .results
                .is_empty(),
            "cancelled coordinator releases mutable results"
        );
    })
    .await
    .expect("coordinator build qualification exceeded 180 seconds");
}

#[tokio::test]
async fn source_loss_hides_results_and_restart_reuses_vectors_without_inferring_deletion() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let registration = registration("records", IndexingMode::Content);
        db.a.register_knowledge_collection(&registration, None)
            .await
            .unwrap();
        let embedding = SyntheticEmbeddings::new();
        let spec = specification(&registration, &embedding);
        let source = ObservedSource {
            inner: Source(Mutex::new(
                [
                    (uri("a"), record(&registration, "facility north")),
                    (uri("b"), record(&registration, "facility south")),
                ]
                .into(),
            )),
            events: broadcast::channel(32).0,
            listening: AtomicUsize::new(0),
            pause_once: AtomicBool::new(false),
            reading: Notify::new(),
            hold: Semaphore::new(0),
        };
        let coordinator = Coordinator {
            store: &db.a,
            source: &source,
            embeddings: &embedding,
        };
        let (sender, mut status) = watch::channel(CoordinatorState::Starting);
        let (outcome, original) = tokio::join!(
            coordinator.run(
                &registration.tenant,
                std::slice::from_ref(&registration),
                &spec,
                CancellationToken::new(),
                sender
            ),
            async {
                let id = ready(&mut status).await;
                source.events.send(true).unwrap();
                id
            }
        );
        assert!(matches!(outcome, Err(ServiceError::SourceUnavailable)));
        assert_eq!(*status.borrow(), CoordinatorState::Failed);
        assert_eq!(embedding.inputs.lock().unwrap().len(), 2);
        let search = SearchService {
            store: &db.b,
            embeddings: &embedding,
        };
        let reader = caller(std::slice::from_ref(&registration));
        let request = SearchRequest::new(
            EmbeddingText::new("facility").unwrap(),
            BTreeSet::new(),
            BTreeSet::new(),
            3,
        )
        .unwrap();
        assert!(
            search
                .search(&reader, &request)
                .await
                .unwrap()
                .results
                .is_empty()
        );
        source.inner.0.lock().unwrap().remove(&uri("b"));
        let cancel = CancellationToken::new();
        let (sender, mut status) = watch::channel(CoordinatorState::Starting);
        let (outcome, _) = tokio::join!(
            coordinator.run(
                &registration.tenant,
                std::slice::from_ref(&registration),
                &spec,
                cancel.clone(),
                sender
            ),
            async {
                assert_eq!(
                    ready(&mut status).await,
                    original,
                    "restart reuses the matching generation"
                );
                assert_eq!(
                    embedding.inputs.lock().unwrap().len(),
                    2,
                    "conditional reads reuse existing vectors"
                );
                let results = search.search(&reader, &request).await.unwrap().results;
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].uri, uri("a"));
                let mut response =
                    db.b.client()
                        .query(include_str!("queries/coordination/source_loss_hides_results_and_restart_reuses_vectors_without_inferring_deletion.surql"))
                        .bind(("uri", uri("b").to_string()))
                        .await
                        .unwrap()
                        .check()
                        .unwrap();
                assert_eq!(
                    response.take::<Vec<bool>>(0).unwrap(),
                    [false],
                    "absence hides a member without claiming deletion"
                );
                // A title change requires new metadata/embeddings even with the same revision.
                source
                    .inner
                    .0
                    .lock()
                    .unwrap()
                    .get_mut(&uri("a"))
                    .unwrap()
                    .title = MemberTitle::new("Renamed facility").unwrap();
                source.pause_once.store(true, Ordering::SeqCst);
                source.events.send(false).unwrap();
                source.reading.notified().await;
                assert_eq!(*status.borrow(), CoordinatorState::Updating(original));
                let readiness =
                    veoveo_knowledge_mcp::indexing::IndexingReadiness::new(vec![status.clone()])
                        .unwrap();
                assert!(readiness.is_ready());
                assert!(
                    search
                        .search(&reader, &request)
                        .await
                        .unwrap()
                        .results
                        .is_empty(),
                    "invalidated members stay hidden while the HTTP endpoint serves updates"
                );
                source.hold.add_permits(1);
                ready(&mut status).await;
                assert_eq!(embedding.inputs.lock().unwrap().len(), 3);
                assert_eq!(
                    search.search(&reader, &request).await.unwrap().results[0]
                        .title
                        .as_str(),
                    "Renamed facility"
                );
                // An owner that changes access without changing revision violates
                // K10. Conditional reuse must reject it before restoring chunks.
                source
                    .inner
                    .0
                    .lock()
                    .unwrap()
                    .get_mut(&uri("a"))
                    .unwrap()
                    .access
                    .data_labels
                    .push("secret".parse().unwrap());
                source.events.send(false).unwrap();
            }
        );
        assert!(matches!(outcome, Err(ServiceError::SourceContract(_))));
        assert!(
            search
                .search(&reader, &request)
                .await
                .unwrap()
                .results
                .is_empty()
        );
        assert_eq!(source.listening.load(Ordering::SeqCst), 2);
    })
    .await
    .expect("coordinator recovery qualification exceeded 180 seconds");
}
