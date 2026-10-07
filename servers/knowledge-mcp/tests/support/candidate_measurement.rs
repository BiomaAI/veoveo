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
const MAX_CAPTURE_BYTES: usize = 512 * 1024 * 1024;

fn serialize_bounded(value: &impl Serialize, limit: usize) -> Result<Vec<u8>> {
    struct Output {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl Write for Output {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::other("candidate output exceeds byte cap"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = Output {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer_pretty(&mut output, value)?;
    Ok(output.bytes)
}

fn write_new(path: PathBuf, value: &impl Serialize) -> Result<()> {
    ensure!(path.is_absolute(), "output must be absolute");
    let bytes = serialize_bounded(value, MAX_CAPTURE_BYTES)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(&bytes)?;
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

#[path = "../../../../testing/fixtures/embedding.rs"]
mod embedding_fixture;

#[test]
fn collector_fixture_is_produced_by_current_checked_owners() -> Result<()> {
    let source = corpus::domain_corpus()?;
    let dataset = source.dataset()?;
    let members = dataset.corpus().to_vec();
    let space = EmbeddingSpace {
        model: EmbeddingModelId::parse("synthetic-fixture")?,
        revision: EmbeddingModelRevision::parse("fixture-1")?,
        dimension: EmbeddingDimension::new(3)?,
        pooling: EmbeddingPooling::LastToken,
        normalization: EmbeddingNormalization::L2,
        precision: EmbeddingPrecision::Float32,
        max_input_tokens: EmbeddingMaxInputTokens::new(8192)?,
    };
    let runtime = embedding_fixture::runtime(space);
    let values = EmbeddingVector::new(&runtime, vec![1.0, 0.0, 0.0])?
        .values()
        .to_vec();
    let query_task = EmbeddingTask::new("Retrieve relevant operational records.")?;
    let chunking = ChunkSettings::new("structure-v1", 1500, 150)?;
    let chunks = members
        .iter()
        .enumerate()
        .map(|(member, _)| {
            Ok(ChunkMeasurement {
                member,
                text: EmbeddingText::new("Synthetic collector admission text.")?,
                values: values.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let queries = dataset
        .cases()
        .iter()
        .map(|case| QueryMeasurement {
            id: case.id().clone(),
            text: case.query().clone(),
            values: values.clone(),
            selected_members: members
                .iter()
                .enumerate()
                .filter(|(_, member)| {
                    case.collections().is_empty()
                        || case.collections().contains(&member.member.collection)
                })
                .map(|(index, _)| index)
                .collect(),
            relevant_members: members
                .iter()
                .enumerate()
                .filter(|(_, member)| case.relevant().contains(&member.member))
                .map(|(index, _)| index)
                .collect(),
        })
        .collect();
    let capture = Capture {
        format: "veoveo.ai/embedding-candidate-capture/v1",
        profile: runtime.profile().clone(),
        query_task_revision: corpus::fingerprint(&query_task),
        chunking_revision: corpus::fingerprint(&chunking),
        query_task,
        chunking,
        dataset_revision: dataset.revision().clone(),
        source_corpus_revision: source.revision(),
        members,
        chunks,
        queries,
        minimum_recall_at_ten: corpus::RecallThreshold::try_from(1.0)
            .map_err(anyhow::Error::msg)?,
    };
    let bytes = serialize_bounded(&capture, MAX_CAPTURE_BYTES)?;
    assert!(serialize_bounded(&capture, bytes.len() - 1).is_err());
    assert_eq!(serialize_bounded(&capture, bytes.len())?, bytes);
    if let Ok(path) = std::env::var("VEOVEO_EMBEDDING_CANDIDATE_FIXTURE_CAPTURE") {
        write_new(PathBuf::from(path), &capture)?;
    } else {
        let expected = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/embedding-candidate-capture.json"
        ))?;
        assert_eq!(
            bytes, expected,
            "collector fixture must come from the current typed producer"
        );
    }
    Ok(())
}

#[test]
fn collector_uri_fixture_is_produced_by_foundation_admission() -> Result<()> {
    #[derive(Serialize)]
    struct Case<'a> {
        uri: &'a str,
        admitted: bool,
    }
    let inputs = [
        ("map://features/a%2Fb?value=%26%3D#part%2f1", true),
        ("a+1.-://owner/path:segment!$&'()*+,;=@?a=b/c?d#part", true),
        ("https://EXAMPLE.com:00443/a/../b?x=%41#fragment", true),
        ("https://[2001:db8::1]:8042/path#fragment", true),
        ("custom://user:password@owner:123456789/path", true),
        ("file:///absolute/path", true),
        ("custom:///path?#", true),
        ("custom://owner:", true),
        ("custom://owner?#", true),
        ("custom://[v1.a]/path", true),
        ("custom://[V1.a]/path", true),
        ("custom://[::ffff:192.168.1.1]:123456789/path", true),
        ("custom://192.168.001.1/path", true),
        ("custom://[::ffff:192.168.001.1]/path", false),
        ("custom://[::ffff:256.168.1.1]/path", false),
        ("custom://owner/%", false),
        ("custom://owner/%2", false),
        ("custom://owner/%GG", false),
        ("custom://owner/空", false),
        ("custom://owner/a b", false),
        ("custom://owner/a\n", false),
        ("custom://owner/\0", false),
        ("custom://owner/\u{007f}", false),
        ("mailto:user@example.com", false),
        ("custom:/path", false),
        ("custom:relative", false),
        ("//owner/path", false),
        ("relative/path", false),
        ("custom://", false),
        ("custom://?#fragment", false),
        ("Custom://owner/path", false),
        ("1custom://owner/path", false),
        ("custom_name://owner/path", false),
        ("custom://[2001:::1]/path", false),
    ];
    let cases = inputs
        .iter()
        .map(|(uri, admitted)| {
            let received = ResourceUri::new(*uri);
            assert_eq!(
                received.is_ok(),
                *admitted,
                "foundation URI admission: {uri:?}"
            );
            if let Ok(received) = received {
                assert_eq!(received.as_str(), *uri, "admission must preserve spelling");
            }
            Case {
                uri,
                admitted: *admitted,
            }
        })
        .collect::<Vec<_>>();
    let bytes = serialize_bounded(&cases, MAX_CAPTURE_BYTES)?;
    if let Ok(path) = std::env::var("VEOVEO_EMBEDDING_URI_FIXTURE_CAPTURE") {
        write_new(PathBuf::from(path), &cases)?;
    } else {
        let expected = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/embedding-collector-uri-cases.json"
        ))?;
        assert_eq!(
            bytes, expected,
            "URI receiver cases come from foundation admission"
        );
    }
    Ok(())
}
