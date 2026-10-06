//! Initial self-qualification from complete hardware reports, not transport success.
use super::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use veoveo_embedding_contract::{EmbeddingQualification, EmbeddingQualificationEvidence};
use veoveo_types::Sha256Digest;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Measurement {
    profile_id: EmbeddingExecutionProfileId,
    space: EmbeddingSpace,
    capture_digest: Sha256Digest,
    dataset_revision: Sha256Digest,
    source_corpus_revision: Sha256Digest,
    query_task_revision: Sha256Digest,
    chunking_revision: Sha256Digest,
    checkpoint_manifest: Sha256Digest,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceReport {
    format: String,
    measurement: Measurement,
    minimum_cosine_millionths: u32,
    passed: bool,
    vectors: usize,
    cuda_device: String,
    reference_precision: String,
    attention_backend: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CaseResult {
    id: String,
    candidate_hits: usize,
    reference_hits: usize,
    relevant: usize,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RetrievalReport {
    format: String,
    measurement: Measurement,
    candidate_recall_at_ten: f64,
    reference_recall_at_ten: f64,
    minimum_recall_at_ten: f64,
    maximum_reference_recall_loss: f64,
    passed: bool,
    cases: Vec<CaseResult>,
    scope: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateSchedulingReport {
    pub format: String,
    pub profile_id: EmbeddingExecutionProfileId,
    pub space: EmbeddingSpace,
    pub query_completed_micros: u64,
    pub bulk_completed_micros: Vec<u64>,
    pub passed: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateCapacityReport {
    pub format: String,
    pub profile_id: EmbeddingExecutionProfileId,
    pub space: EmbeddingSpace,
    pub scheduling_report: Sha256Digest,
    pub bulk_inputs: u64,
    pub elapsed_micros: u64,
    pub query_latency_micros: u64,
    pub minimum_inputs_per_second: f64,
    pub maximum_query_latency_micros: u64,
    pub passed: bool,
}
fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, EmbeddingClientError> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(EmbeddingClientError::Configuration(
            "hardware report exceeds 4 MiB",
        ));
    }
    serde_json::from_slice(bytes)
        .map_err(|_| EmbeddingClientError::Configuration("invalid hardware report"))
}
fn require(value: bool) -> Result<(), EmbeddingClientError> {
    if value {
        Ok(())
    } else {
        Err(EmbeddingClientError::Configuration(
            "hardware reports mismatch or fail acceptance",
        ))
    }
}
fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

/// Produces an initial self-compatible bundle. Installation must still run the full
/// production Knowledge GPU workload before selecting it. Reports are trusted
/// operator inputs; this API does not remotely attest a serving process.
pub fn initial_runtime_from_reports(
    profile: EmbeddingExecutionProfile,
    reference_bytes: &[u8],
    retrieval_bytes: &[u8],
    scheduling_bytes: &[u8],
    capacity_bytes: &[u8],
) -> Result<QualifiedEmbeddingRuntime, EmbeddingClientError> {
    let (_, minimum_cosine_millionths) =
        verify_comparison(&profile, reference_bytes, retrieval_bytes)?;
    let scheduling: CandidateSchedulingReport = decode(scheduling_bytes)?;
    let capacity: CandidateCapacityReport = decode(capacity_bytes)?;
    require(
        scheduling.format == "veoveo.ai/embedding-candidate-scheduling/v1"
            && scheduling.passed
            && &scheduling.profile_id == profile.id()
            && &scheduling.space == profile.space()
            && scheduling.query_completed_micros > 0
            && scheduling.bulk_completed_micros.len() == 6
            && scheduling
                .bulk_completed_micros
                .iter()
                .filter(|time| **time > scheduling.query_completed_micros)
                .count()
                >= 4,
    )?;
    require(
        capacity.format == "veoveo.ai/embedding-candidate-capacity/v1"
            && capacity.passed
            && &capacity.profile_id == profile.id()
            && &capacity.space == profile.space()
            && capacity.scheduling_report == digest(scheduling_bytes)
            && capacity.elapsed_micros
                >= scheduling
                    .bulk_completed_micros
                    .iter()
                    .copied()
                    .max()
                    .unwrap_or(0)
            && capacity.query_latency_micros <= scheduling.query_completed_micros
            && capacity.bulk_inputs == 192
            && capacity.elapsed_micros > 0
            && capacity.query_latency_micros > 0
            && capacity.minimum_inputs_per_second.is_finite()
            && capacity.minimum_inputs_per_second > 0.0
            && capacity.maximum_query_latency_micros > 0
            && capacity.query_latency_micros <= capacity.maximum_query_latency_micros
            && capacity.bulk_inputs as f64 * 1_000_000.0 / capacity.elapsed_micros as f64
                >= capacity.minimum_inputs_per_second,
    )?;
    let qualification = EmbeddingQualification::new(
        &profile,
        &profile,
        EmbeddingQualificationEvidence {
            reference_report: digest(reference_bytes),
            retrieval_report: digest(retrieval_bytes),
            scheduling_report: digest(scheduling_bytes),
            capacity_report: digest(capacity_bytes),
            minimum_cosine_millionths,
            retrieval_passed: true,
            interactive_priority_passed: true,
            capacity_passed: true,
        },
    )?;
    QualifiedEmbeddingRuntime::new(profile.clone(), vec![profile], vec![qualification])
        .map_err(Into::into)
}

#[derive(Debug, Clone)]
pub struct CandidateReportContext {
    pub dataset_revision: Sha256Digest,
    pub source_corpus_revision: Sha256Digest,
    pub query_task_revision: Sha256Digest,
    pub chunking_revision: Sha256Digest,
    pub minimum_recall_at_ten: f64,
}
fn verify_comparison(
    profile: &EmbeddingExecutionProfile,
    reference_bytes: &[u8],
    retrieval_bytes: &[u8],
) -> Result<(CandidateReportContext, u32), EmbeddingClientError> {
    let reference: ReferenceReport = decode(reference_bytes)?;
    let retrieval: RetrievalReport = decode(retrieval_bytes)?;
    for measurement in [&reference.measurement, &retrieval.measurement] {
        require(
            &measurement.profile_id == profile.id()
                && &measurement.space == profile.space()
                && measurement.checkpoint_manifest == profile.contents().checkpoint_manifest,
        )?;
    }
    require(
        reference.format == "veoveo.ai/embedding-candidate-reference/v1"
            && reference.passed
            && (999000..=1000000).contains(&reference.minimum_cosine_millionths)
            && reference.vectors > 0
            && reference.cuda_device == profile.contents().environment.gpu.as_ref()
            && reference.reference_precision == "bfloat16"
            && reference.attention_backend == "cudnn_attention",
    )?;
    require(
        retrieval.format == "veoveo.ai/embedding-candidate-retrieval/v1"
            && retrieval.scope == "candidate_vector_ranking"
            && retrieval.passed
            && reference.measurement.capture_digest == retrieval.measurement.capture_digest
            && reference.measurement.dataset_revision == retrieval.measurement.dataset_revision
            && reference.measurement.source_corpus_revision
                == retrieval.measurement.source_corpus_revision
            && reference.measurement.query_task_revision
                == retrieval.measurement.query_task_revision
            && reference.measurement.chunking_revision == retrieval.measurement.chunking_revision
            && !retrieval.cases.is_empty()
            && retrieval.cases.len() <= 1024
            && retrieval.minimum_recall_at_ten.is_finite()
            && retrieval.minimum_recall_at_ten > 0.0
            && retrieval.minimum_recall_at_ten <= 1.0
            && retrieval.maximum_reference_recall_loss == 0.0,
    )?;
    let mut ids = std::collections::BTreeSet::new();
    let mut candidate_recall = 0.0;
    let mut reference_recall = 0.0;
    for case in &retrieval.cases {
        require(
            !case.id.is_empty()
                && case.id.len() <= 64
                && ids.insert(&case.id)
                && case.relevant > 0
                && case.candidate_hits <= case.relevant.min(10)
                && case.reference_hits <= case.relevant.min(10),
        )?;
        candidate_recall += case.candidate_hits as f64 / case.relevant as f64;
        reference_recall += case.reference_hits as f64 / case.relevant as f64;
    }
    candidate_recall /= retrieval.cases.len() as f64;
    reference_recall /= retrieval.cases.len() as f64;
    require(
        (candidate_recall - retrieval.candidate_recall_at_ten).abs() < 1e-12
            && (reference_recall - retrieval.reference_recall_at_ten).abs() < 1e-12
            && candidate_recall >= retrieval.minimum_recall_at_ten
            && candidate_recall >= reference_recall,
    )?;

    Ok((
        CandidateReportContext {
            dataset_revision: retrieval.measurement.dataset_revision,
            source_corpus_revision: retrieval.measurement.source_corpus_revision,
            query_task_revision: retrieval.measurement.query_task_revision,
            chunking_revision: retrieval.measurement.chunking_revision,
            minimum_recall_at_ten: retrieval.minimum_recall_at_ten,
        },
        reference.minimum_cosine_millionths,
    ))
}
/// Recovers the measured comparison context only when the bundle binds these report bytes.
pub fn candidate_context_from_reports(
    runtime: &QualifiedEmbeddingRuntime,
    reference_bytes: &[u8],
    retrieval_bytes: &[u8],
) -> Result<CandidateReportContext, EmbeddingClientError> {
    let evidence = runtime
        .qualification_for(runtime.profile().id())?
        .evidence();
    require(
        evidence.reference_report == digest(reference_bytes)
            && evidence.retrieval_report == digest(retrieval_bytes),
    )?;
    verify_comparison(runtime.profile(), reference_bytes, retrieval_bytes)
        .map(|(context, _)| context)
}
