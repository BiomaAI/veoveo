//! An isolated RocksDB rebuild and retrieval workload against a qualified CUDA
//! runtime. Source captures are fixtures; this does not certify installed sources.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
use veoveo_embedding_client::{EmbeddingClient, EmbeddingClientConfig, EmbeddingEndpoint};
use veoveo_embedding_contract::*;
use veoveo_knowledge_mcp::{
    ServiceError, access::SearchCaller, contract::*, embed::Embeddings,
    evaluation::RetrievalEvaluator, index::Indexer, search::SearchService, source::*,
};
use veoveo_mcp_knowledge_extension::{CollectionDescriptor, CollectionId, Observation};
use veoveo_types::*;

#[path = "support/retrieval_corpus.rs"]
mod corpus;
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Configuration {
    space: EmbeddingSpace,
    query_task: EmbeddingTask,
    chunking: ChunkSettings,
    corpus: corpus::Corpus,
}

#[test]
#[ignore = "writes three model configurations to VEOVEO_RETRIEVAL_FIXTURE_DIR"]
fn write_domain_comparison_configurations() {
    let directory = PathBuf::from(
        std::env::var("VEOVEO_RETRIEVAL_FIXTURE_DIR").expect("set VEOVEO_RETRIEVAL_FIXTURE_DIR"),
    );
    assert!(
        directory.is_absolute(),
        "fixture directory must be absolute"
    );
    std::fs::create_dir_all(&directory).unwrap();
    let corpus = corpus::domain_corpus().unwrap();
    for (name, revision, dimension) in [
        (
            "qwen3-embedding-0.6b",
            "97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3",
            1024,
        ),
        (
            "qwen3-embedding-4b",
            "5cf2132abc99cad020ac570b19d031efec650f2b",
            2560,
        ),
        (
            "qwen3-embedding-8b",
            "1d8ad4ca9b3dd8059ad90a75d4983776a23d44af",
            4096,
        ),
    ] {
        let config = Configuration {
            space: EmbeddingSpace {
                model: EmbeddingModelId::new(name).unwrap(),
                revision: EmbeddingModelRevision::new(revision).unwrap(),
                dimension: EmbeddingDimension::new(dimension).unwrap(),
                runtime_image:
                    "sha256:8a69ffad015f138d7170c4ddc429e230a3bc1c1719f67e14324749df200a4b90"
                        .parse()
                        .unwrap(),
            },
            query_task: EmbeddingTask::new(
                "Given a web search query, retrieve relevant passages that answer the query",
            )
            .unwrap(),
            chunking: ChunkSettings::new("structure-v1", 1500, 150).unwrap(),
            corpus: corpus.clone(),
        };
        let path = directory.join(format!("{name}.json"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.write_all(&serde_json::to_vec_pretty(&config).unwrap())
            .unwrap();
        file.sync_all().unwrap();
    }
    println!(
        "{}",
        serde_json::json!({"directory":directory,"datasetRevision":corpus.dataset().unwrap().revision(),"sourceCorpusRevision":corpus.revision(),"members":corpus.members.len(),"cases":corpus.cases.len()})
    );
}

struct CountedEmbeddings {
    client: EmbeddingClient,
    chunks: AtomicU64,
}
impl Embeddings for CountedEmbeddings {
    fn space(&self) -> &EmbeddingSpace {
        self.client.space()
    }
    async fn documents(&self, texts: EmbeddingBatch) -> Result<Vec<EmbeddingVector>, ServiceError> {
        let result = Embeddings::documents(&self.client, texts).await?;
        self.chunks
            .fetch_add(result.len() as u64, Ordering::Relaxed);
        Ok(result)
    }
    async fn queries(
        &self,
        task: EmbeddingTask,
        texts: EmbeddingBatch,
    ) -> Result<Vec<EmbeddingVector>, ServiceError> {
        Embeddings::queries(&self.client, task, texts).await
    }
    async fn query(
        &self,
        task: EmbeddingTask,
        text: EmbeddingText,
    ) -> Result<EmbeddingVector, ServiceError> {
        Embeddings::query(&self.client, task, text).await
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CollectionTiming {
    collection: CollectionId,
    chunks: u64,
    elapsed_micros: u64,
    chunks_per_second: f64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RebuildMeasurement {
    generation: GenerationId,
    collections: Vec<CollectionTiming>,
    elapsed_micros: u64,
    chunks_per_second: f64,
    concurrent_searches: usize,
    search_p50_micros: u64,
    search_p95_micros: u64,
    search_max_micros: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    format: &'static str,
    specification: GenerationSpec,
    dataset_revision: Sha256Digest,
    source_corpus_revision: Sha256Digest,
    rebuild: RebuildMeasurement,
    recall_at_ten: f64,
    case_recall: BTreeMap<EvaluationCaseId, RecallCounts>,
    evaluation_revision: Sha256Digest,
    evaluation: RetrievalEvaluation,
}

#[tokio::test]
#[ignore = "requires a judged source corpus and a separately qualified hardware CUDA embedding runtime"]
async fn domain_recall_and_rebuild_with_concurrent_searches() {
    tokio::time::timeout(Duration::from_secs(1800), run())
        .await
        .expect("GPU retrieval benchmark exceeded 30 minutes")
        .unwrap();
}

async fn run() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let input = PathBuf::from(
        std::env::var("VEOVEO_RETRIEVAL_INPUT")
            .context("set VEOVEO_RETRIEVAL_INPUT to an absolute configuration path")?,
    );
    let output = PathBuf::from(
        std::env::var("VEOVEO_RETRIEVAL_OUTPUT")
            .context("set VEOVEO_RETRIEVAL_OUTPUT to a new absolute report path")?,
    );
    ensure!(
        input.is_absolute() && output.is_absolute() && !output.exists(),
        "input/output paths must be absolute and the report must be new"
    );
    ensure!(
        std::fs::metadata(&input)?.len() <= 32 * 1024 * 1024,
        "evaluation configuration exceeds 32 MiB"
    );
    let config: Configuration = serde_json::from_slice(&std::fs::read(input)?)?;
    let source = config.corpus.validate()?;
    let dataset = config.corpus.dataset()?;
    let caller = config.corpus.caller()?;
    ensure!(
        dataset.corpus().len() > 10 && dataset.cases().len() >= 10,
        "GPU comparison requires at least 11 members and 10 judged queries"
    );
    ensure!(
        dataset.cases().iter().all(|case| dataset
            .corpus()
            .iter()
            .filter(|m| case.collections().is_empty()
                || case.collections().contains(&m.member.collection))
            .count()
            > 10),
        "each query must select more than ten candidate members for recall at ten to discriminate models"
    );
    let endpoint = EmbeddingEndpoint::parse(
        &std::env::var("VEOVEO_EMBEDDING_URL").context("set VEOVEO_EMBEDDING_URL")?,
    )?;
    let key_path = std::env::var("VEOVEO_EMBEDDING_API_KEY_FILE")
        .context("set VEOVEO_EMBEDDING_API_KEY_FILE")?;
    let key = secrecy::SecretString::from(
        std::fs::read_to_string(key_path).context("read embedding key file")?,
    );
    let embeddings = CountedEmbeddings {
        client: EmbeddingClient::connect(EmbeddingClientConfig::new(
            endpoint,
            key,
            config.space.clone(),
        ))
        .await?,
        chunks: AtomicU64::new(0),
    };
    let db = fixture::TestDb::with_backend(fixture::StoreBackend::RocksDb).await;
    let lease =
        db.a.claim_knowledge_coordinator(&caller.tenant, Default::default())
            .await?
            .context("isolated indexing lease unavailable")?;
    for registration in &config.corpus.registrations {
        db.a.register_knowledge_collection(registration, None)
            .await?;
    }
    let spec = GenerationSpec::new(
        config.space,
        config.query_task.as_str(),
        config.chunking,
        config
            .corpus
            .registrations
            .iter()
            .map(|r| (r.descriptor.collection().clone(), r.revision()))
            .collect(),
    )?;
    let workload = async {
        let indexer = Indexer {
            store: &db.a,
            lease: &lease,
            source: &source,
            embeddings: &embeddings,
        };
        let initial = indexer
            .build(&caller.tenant, &config.corpus.registrations, &spec)
            .await?;
        db.a.activate_knowledge_generation(&lease, &caller.tenant, initial, None)
            .await?;
        // Warm the actual search path before load measurement. It also proves
        // that the source fixture and judged corpus agree under caller policy.
        RetrievalEvaluator {
            store: &db.b,
            embeddings: &embeddings,
        }
        .evaluate(&caller, &dataset)
        .await?;
        let began = Instant::now();
        let next = indexer
            .prepare(&caller.tenant, &config.corpus.registrations, &spec)
            .await?;
        let stop = CancellationToken::new();
        let rebuild = async {
            let result = async {
                let mut collections = Vec::new();
                for registration in &config.corpus.registrations {
                    let before = embeddings.chunks.load(Ordering::Relaxed);
                    let start = Instant::now();
                    indexer.reconcile(registration, next, &spec).await?;
                    let elapsed = start.elapsed();
                    let chunks = embeddings.chunks.load(Ordering::Relaxed) - before;
                    collections.push(CollectionTiming {
                        collection: registration.descriptor.collection().clone(),
                        chunks,
                        elapsed_micros: micros(elapsed),
                        chunks_per_second: chunks as f64 / elapsed.as_secs_f64(),
                    });
                }
                Ok::<_, anyhow::Error>(collections)
            }
            .await;
            stop.cancel();
            result
        };
        let searches = concurrent_searches(&db.b, &embeddings, &caller, &dataset, initial, &stop);
        let (collections, mut latencies) = tokio::try_join!(rebuild, searches)?;
        ensure!(
            !latencies.is_empty(),
            "rebuild completed without a concurrent search"
        );
        latencies.sort_unstable();
        let chunks: u64 = collections.iter().map(|c| c.chunks).sum();
        db.a.activate_knowledge_generation(&lease, &caller.tenant, next, Some(initial))
            .await?;
        let elapsed = began.elapsed();
        let evaluation = RetrievalEvaluator {
            store: &db.b,
            embeddings: &embeddings,
        }
        .evaluate(&caller, &dataset)
        .await?;
        let evaluation_revision =
            db.a.record_knowledge_evaluation(&caller.tenant, &evaluation)
                .await?;
        let persisted =
            db.b.knowledge_evaluation(&caller.tenant, next, &evaluation_revision)
                .await?
                .context("evaluation was not readable from the second connection")?;
        ensure!(
            persisted.revision() == evaluation_revision,
            "stored evaluation differs"
        );
        let report = Report {
            format: "veoveo.ai/knowledge-retrieval-evaluation/v1",
            specification: spec.clone(),
            dataset_revision: dataset.revision(),
            source_corpus_revision: config.corpus.revision(),
            rebuild: RebuildMeasurement {
                generation: next,
                collections,
                elapsed_micros: micros(elapsed),
                chunks_per_second: chunks as f64 / elapsed.as_secs_f64(),
                concurrent_searches: latencies.len(),
                search_p50_micros: percentile(&latencies, 50),
                search_p95_micros: percentile(&latencies, 95),
                search_max_micros: *latencies.last().unwrap(),
            },
            recall_at_ten: evaluation.recall_at_ten(),
            case_recall: evaluation.case_recall(),
            evaluation_revision,
            evaluation,
        };
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&output)?;
        file.write_all(&serde_json::to_vec_pretty(&report)?)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        println!(
            "{}",
            serde_json::json!({"report":output,"recallAt10":report.recall_at_ten,"chunksPerSecond":report.rebuild.chunks_per_second,"concurrentSearches":report.rebuild.concurrent_searches})
        );
        Ok::<_, anyhow::Error>(())
    };
    // The production coordinator has the same lease renewal obligation. Keep
    // renewal active through both rebuilds and evaluation; no extra worker leaks.
    tokio::pin!(workload);
    let mut renewal = tokio::time::interval(Duration::from_secs(10));
    loop {
        tokio::select! {
            result = &mut workload => { db.a.release_knowledge_coordinator(&lease).await?; return result; }
            _ = renewal.tick() => db.a.renew_knowledge_coordinator(&lease).await?,
        }
    }
}

async fn concurrent_searches(
    store: &veoveo_platform_store::PlatformStore,
    embeddings: &CountedEmbeddings,
    caller: &SearchCaller,
    dataset: &RetrievalDataset,
    generation: GenerationId,
    stop: &CancellationToken,
) -> Result<Vec<u64>> {
    let search = SearchService { store, embeddings };
    let mut latencies = Vec::new();
    let mut arrival = tokio::time::interval(Duration::from_millis(100));
    arrival.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            _ = stop.cancelled() => return Ok(latencies),
            _ = arrival.tick() => {}
        }
        let case = &dataset.cases()[latencies.len() % dataset.cases().len()];
        let selected = if case.collections().is_empty() {
            dataset.collections()
        } else {
            case.collections().clone()
        };
        let request = SearchRequest::new(case.query().clone(), selected, Default::default(), 10)?;
        let began = Instant::now();
        let result = search.search(caller, &request).await?;
        ensure!(
            result.generation == Some(generation) && !result.results.is_empty(),
            "concurrent search lost the active generation or visible corpus"
        );
        latencies.push(micros(began.elapsed()));
    }
}
fn micros(duration: Duration) -> u64 {
    duration.as_micros().max(1) as u64
}
fn percentile(sorted: &[u64], percent: usize) -> u64 {
    sorted[(sorted.len() * percent).div_ceil(100) - 1]
}
