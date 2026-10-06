//! Candidate-only capture: no Store, generation, activation or Embeddings implementation.
use super::*;
use veoveo_embedding_client::verification::{CandidateClientConfig, CandidateEmbeddingClient};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CandidateConfiguration {
    profile: EmbeddingExecutionProfile,
    query_task: EmbeddingTask,
    chunking: ChunkSettings,
    corpus: corpus::Corpus,
    minimum_recall_at_ten: corpus::RecallThreshold,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChunkMeasurement {
    member: usize,
    text: EmbeddingText,
    values: Vec<f32>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryMeasurement {
    id: EvaluationCaseId,
    text: EmbeddingText,
    values: Vec<f32>,
    selected_members: Vec<usize>,
    relevant_members: Vec<usize>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Capture {
    format: &'static str,
    profile: EmbeddingExecutionProfile,
    query_task: EmbeddingTask,
    chunking: ChunkSettings,
    dataset_revision: Sha256Digest,
    source_corpus_revision: Sha256Digest,
    query_task_revision: Sha256Digest,
    chunking_revision: Sha256Digest,
    members: Vec<EvaluationMember>,
    chunks: Vec<ChunkMeasurement>,
    queries: Vec<QueryMeasurement>,
    minimum_recall_at_ten: corpus::RecallThreshold,
}
fn write_new(path: PathBuf, value: &impl Serialize) -> Result<()> {
    ensure!(path.is_absolute(), "output must be absolute");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    Ok(())
}
#[test]
#[ignore = "writes candidate measurement configuration from an actual measured NVIDIA profile"]
fn write_candidate_domain_configuration() -> Result<()> {
    let profile: EmbeddingExecutionProfile = serde_json::from_slice(&std::fs::read(
        std::env::var("VEOVEO_EMBEDDING_PROFILE_FILE").context("set measured profile file")?,
    )?)?;
    let minimum: f64 = std::env::var("VEOVEO_EMBEDDING_MINIMUM_RECALL_AT_TEN")
        .context("set installation acceptance threshold")?
        .parse()?;
    let minimum = corpus::RecallThreshold::try_from(minimum).map_err(anyhow::Error::msg)?;
    write_new(
        PathBuf::from(std::env::var("VEOVEO_RETRIEVAL_INPUT")?),
        &CandidateConfiguration {
            profile,
            query_task: EmbeddingTask::new(
                "Given a web search query, retrieve relevant passages that answer the query",
            )?,
            chunking: ChunkSettings::new("structure-v1", 1500, 150)?,
            corpus: corpus::domain_corpus()?,
            minimum_recall_at_ten: minimum,
        },
    )
}
#[tokio::test]
#[ignore = "requires measured NVIDIA candidate endpoint; outputs vectors only, not production SQL acceptance"]
async fn capture_candidate_domain_vectors() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(1800), async {
        let input = PathBuf::from(std::env::var("VEOVEO_RETRIEVAL_INPUT")?);
        ensure!(input.is_absolute(), "input must be absolute");
        ensure!(
            std::fs::metadata(&input)?.len() <= 32 * 1024 * 1024,
            "candidate configuration exceeds 32 MiB"
        );
        let config: CandidateConfiguration = serde_json::from_slice(&std::fs::read(input)?)?;
        let _ = config.corpus.validate()?;
        let _ = config.corpus.caller()?;
        let dataset = config.corpus.dataset()?;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = CandidateEmbeddingClient::connect(CandidateClientConfig::new(
            EmbeddingEndpoint::parse(&std::env::var("VEOVEO_EMBEDDING_URL")?)?,
            secrecy::SecretString::from(std::fs::read_to_string(std::env::var(
                "VEOVEO_EMBEDDING_API_KEY_FILE",
            )?)?),
            config.profile.clone(),
        ))
        .await?;
        let members: Vec<_> = dataset.corpus().to_vec();
        let mut chunks = Vec::new();
        for member in &config.corpus.members {
            let member_index = members
                .iter()
                .position(|m| m.member == member.identity())
                .context("source identity missing from dataset")?;
            for range in veoveo_knowledge_mcp::chunk::ranges(member.text(), &config.chunking)? {
                let text = EmbeddingText::new(member.text()[range.start..range.end].to_owned())?;
                let batch = EmbeddingBatch::new(vec![text.clone()])?;
                let measurement = client
                    .embed_documents(&batch, EmbeddingPriority::Bulk)
                    .await?
                    .remove(0);
                chunks.push(ChunkMeasurement {
                    member: member_index,
                    text,
                    values: measurement.values().to_vec(),
                });
            }
        }
        let mut queries = Vec::new();
        for case in dataset.cases() {
            let measured = client.embed_query(&config.query_task, case.query()).await?;
            queries.push(QueryMeasurement {
                id: case.id().clone(),
                text: case.query().clone(),
                values: measured.values().to_vec(),
                selected_members: members
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| {
                        case.collections().is_empty()
                            || case.collections().contains(&m.member.collection)
                    })
                    .map(|(index, _)| index)
                    .collect(),
                relevant_members: members
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| case.relevant().contains(&m.member))
                    .map(|(index, _)| index)
                    .collect(),
            });
        }
        write_new(
            PathBuf::from(std::env::var("VEOVEO_RETRIEVAL_OUTPUT")?),
            &Capture {
                format: "veoveo.ai/embedding-candidate-capture/v1",
                profile: config.profile,
                query_task_revision: corpus::fingerprint(&config.query_task),
                chunking_revision: corpus::fingerprint(&config.chunking),
                query_task: config.query_task,
                chunking: config.chunking,
                dataset_revision: dataset.revision().clone(),
                source_corpus_revision: config.corpus.revision(),
                members,
                chunks,
                queries,
                minimum_recall_at_ten: config.minimum_recall_at_ten,
            },
        )
    })
    .await
    .context("candidate capture exceeded 1800 seconds")?
}
