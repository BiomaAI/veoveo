//! Installed GPU acceptance. The caller starts the pinned, CUDA-refusing runtime.
use serde::Deserialize;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::time::timeout;
use veoveo_embedding_client::*;

use veoveo_embedding_contract::{
    EmbeddingError, EmbeddingGpuName, EmbeddingPrecision, EmbeddingRuntimeVersion,
    validate_embedding_values,
};
use veoveo_types::{Check, Checked, Sha256Digest};

#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
enum ReferenceSchema {
    #[vocabulary(rename = "veoveo.ai/embedding-reference/v2")]
    V2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
enum ReferenceAttention {
    #[vocabulary(rename = "cudnn_attention")]
    CudnnAttention,
}

/// Basic Python reference output; candidate measurement reports have another owner.
type Reference = Checked<ReferenceWire>;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceWire {
    schema: ReferenceSchema,
    space: EmbeddingSpace,
    checkpoint_manifest: Sha256Digest,
    generator: ReferenceGenerator,
    task: EmbeddingTask,
    queries: Vec<Sample>,
    documents: Vec<Sample>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceGenerator {
    torch: EmbeddingRuntimeVersion,
    transformers: EmbeddingRuntimeVersion,
    cuda: EmbeddingRuntimeVersion,
    gpu: EmbeddingGpuName,
    attention: ReferenceAttention,
    dtype: EmbeddingPrecision,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sample {
    text: EmbeddingText,
    values: Vec<f32>,
}
impl Check for ReferenceWire {
    type Error = EmbeddingError;
    fn check(&self) -> Result<(), Self::Error> {
        if !matches!(
            self.generator.dtype,
            EmbeddingPrecision::Bfloat16 | EmbeddingPrecision::Float16
        ) || self.generator.dtype != self.space.precision
        {
            return Err(EmbeddingError(
                "reference precision must match its supported embedding space",
            ));
        }
        if self.queries.is_empty() || self.documents.is_empty() {
            return Err(EmbeddingError(
                "reference requires query and document samples",
            ));
        }
        for sample in self.queries.iter().chain(&self.documents) {
            validate_embedding_values(&self.space, &sample.values)?;
        }
        Ok(())
    }
}

async fn fixture() -> (EmbeddingClient, Reference) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let path = PathBuf::from(
        std::env::var("VEOVEO_EMBEDDING_REFERENCE_FILE")
            .expect("set reference file with explicit current vector space"),
    );
    let reference: Reference = serde_json::from_slice(
        &std::fs::read(path).expect("generate the GPU reference fixture first"),
    )
    .unwrap();
    let endpoint = EmbeddingEndpoint::parse(
        &std::env::var("VEOVEO_EMBEDDING_URL")
            .expect("set VEOVEO_EMBEDDING_URL to the qualified CUDA runtime"),
    )
    .unwrap();
    let key_path =
        std::env::var("VEOVEO_EMBEDDING_API_KEY_FILE").expect("set VEOVEO_EMBEDDING_API_KEY_FILE");
    let key = secrecy::SecretString::from(
        std::fs::read_to_string(key_path).expect("read embedding key file"),
    );
    let runtime: QualifiedEmbeddingRuntime = serde_json::from_slice(
        &std::fs::read(
            std::env::var("VEOVEO_EMBEDDING_RUNTIME_FILE").expect("set measured runtime bundle"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(runtime.space(), &reference.space);
    let client = EmbeddingClient::connect(EmbeddingClientConfig::new(endpoint, key, runtime))
        .await
        .unwrap();
    (client, reference)
}

fn cosine(a: &[f32], b: &[f32]) -> f64 {
    assert_eq!(a.len(), b.len());
    let dot: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum();
    let norm = |v: &[f32]| v.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>().sqrt();
    dot / (norm(a) * norm(b))
}

#[tokio::test]
#[ignore = "requires the pinned CUDA embedding runtime and generated reference fixture"]
async fn cuda_runtime_matches_transformers_reference_vectors() {
    let (client, reference) = fixture().await;
    timeout(Duration::from_secs(120), async {
        let documents = EmbeddingBatch::new(reference.documents.iter().map(|s| s.text.clone()).collect()).unwrap();
        let queries = EmbeddingBatch::new(reference.queries.iter().map(|s| s.text.clone()).collect()).unwrap();
        let actual_docs = client.embed_documents(&documents, EmbeddingPriority::Bulk).await.unwrap();
        let actual_queries = client.embed_queries(&reference.task, &queries, EmbeddingPriority::Interactive).await.unwrap();
        let similarities: Vec<_> = actual_docs.iter().zip(&reference.documents).chain(actual_queries.iter().zip(&reference.queries))
            .map(|(actual, expected)| { assert_eq!(actual.space(), &reference.space); cosine(actual.values(), &expected.values) }).collect();
        assert!(similarities.iter().all(|score| *score >= 0.999), "reference cosine similarities: {similarities:?}");
        println!("{}", serde_json::json!({"test":"embedding_reference", "result":"pass", "cosine":similarities,"space":reference.space}));
    }).await.expect("reference comparison exceeded 120 seconds");
}

#[tokio::test]
#[ignore = "requires the pinned CUDA embedding runtime and generated reference fixture"]
async fn interactive_query_completes_ahead_of_queued_bulk_work_on_cuda() {
    let (client, reference) = fixture().await;
    timeout(Duration::from_secs(120), async {
        let text = EmbeddingText::new("Inspect the flood boundary, shelter capacity and bridge access. ".repeat(60)).unwrap();
        let batch = EmbeddingBatch::new(vec![text; 32]).unwrap();
        let began = Instant::now();
        let mut pending = tokio::task::JoinSet::new();
        for index in 0..6 {
            let client = client.clone(); let batch = batch.clone();
            pending.spawn(async move {
                client.embed_documents(&batch, EmbeddingPriority::Bulk).await.unwrap();
                (index, began.elapsed().as_millis())
            });
        }
        // Queue bulk work before submitting the interactive query. This is a
        // load arrival offset, not a readiness or completion polling loop.
        tokio::time::sleep(Duration::from_millis(25)).await;
        let query_began = Instant::now();
        client.embed_query(&reference.task, &reference.queries[0].text).await.unwrap();
        let query_ms = query_began.elapsed().as_millis();
        let query_done = began.elapsed().as_millis();
        let mut completed = Vec::new();
        while let Some(result) = pending.join_next().await { completed.push(result.unwrap()); }
        assert!(completed.iter().filter(|(_, ended)| *ended > query_done).count() >= 4,
            "interactive completion {query_done} ms did not precede queued bulk: {completed:?}");
        let elapsed = began.elapsed().as_secs_f64();
        println!("{}", serde_json::json!({"test":"embedding_priority", "result":"pass", "queryLatencyMs":query_ms,"queryCompletedMs":query_done,"bulkCompletedMs":completed,"bulkInputsPerSecond":192.0/elapsed}));
    }).await.expect("GPU scheduling check exceeded 120 seconds");
}

#[cfg(feature = "verification")]
#[path = "gpu/candidate.rs"]
mod candidate;

#[test]
fn basic_reference_refuses_unsupported_schema_before_runtime_setup() {
    let mut wire: serde_json::Value =
        serde_json::from_str(include_str!("../../verification/reference.json")).unwrap();
    wire["schema"] = "veoveo.ai/embedding-reference/v1".into();
    assert!(serde_json::from_value::<Reference>(wire).is_err());
}

fn basic_reference_wire() -> serde_json::Value {
    serde_json::from_str(include_str!("../../verification/reference.json")).unwrap()
}

#[test]
fn basic_reference_admits_actual_v2_generator_and_samples() {
    let reference: Reference = serde_json::from_value(basic_reference_wire()).unwrap();
    assert_eq!(reference.schema, ReferenceSchema::V2);
    assert_eq!(
        reference.checkpoint_manifest.hex(),
        "2f0df70974121b6286d288c6b8639b576c4157fc7499fff717984cce3ab72323"
    );
    assert_eq!(reference.generator.torch.as_ref(), "2.13.0+cu130");
    assert_eq!(reference.generator.transformers.as_ref(), "5.17.0");
    assert_eq!(reference.generator.cuda.as_ref(), "13.0");
    assert_eq!(reference.generator.gpu.as_ref(), "NVIDIA GeForce RTX 4090");
    assert_eq!(
        reference.generator.attention,
        ReferenceAttention::CudnnAttention
    );
    assert_eq!(reference.generator.dtype, EmbeddingPrecision::Float16);
    assert_eq!((reference.queries.len(), reference.documents.len()), (2, 2));
    for sample in reference.queries.iter().chain(&reference.documents) {
        assert_eq!(sample.values.len(), 1024);
    }
    let mut bf16 = basic_reference_wire();
    bf16["space"]["precision"] = "bfloat16".into();
    bf16["generator"]["dtype"] = "bfloat16".into();
    assert!(serde_json::from_value::<Reference>(bf16).is_ok());
}

#[test]
fn basic_reference_refuses_missing_retired_mixed_and_candidate_markers() {
    for schema in [
        serde_json::Value::Null,
        "veoveo.ai/embedding-reference/v1".into(),
        "veoveo.ai/embedding-reference/v3".into(),
        "veoveo.ai/embedding-candidate-reference/v1".into(),
    ] {
        let mut wire = basic_reference_wire();
        wire["schema"] = schema;
        assert!(serde_json::from_value::<Reference>(wire).is_err());
    }
    let mut missing = basic_reference_wire();
    missing.as_object_mut().unwrap().remove("schema");
    assert!(serde_json::from_value::<Reference>(missing).is_err());
    for key in ["format", "checkpoint_manifest", "unexpected"] {
        let mut wire = basic_reference_wire();
        wire[key] = "synthetic-unadmitted".into();
        assert!(serde_json::from_value::<Reference>(wire).is_err());
    }
    let mut retired = basic_reference_wire();
    let digest = retired
        .as_object_mut()
        .unwrap()
        .remove("checkpointManifest")
        .unwrap();
    retired["checkpoint_manifest"] = digest;
    assert!(serde_json::from_value::<Reference>(retired).is_err());
    let raw = include_str!("../../verification/reference.json");
    let duplicate = raw.replacen(
        "\"schema\":",
        "\"schema\":\"veoveo.ai/embedding-reference/v1\",\"schema\":",
        1,
    );
    assert!(serde_json::from_str::<Reference>(&duplicate).is_err());
}

#[test]
fn basic_reference_refuses_unadmitted_provenance_and_vector_relationships() {
    for field in [
        "space",
        "checkpointManifest",
        "generator",
        "task",
        "queries",
        "documents",
    ] {
        let mut wire = basic_reference_wire();
        wire.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<Reference>(wire).is_err(),
            "missing {field}"
        );
    }
    for field in ["torch", "transformers", "cuda", "gpu", "attention", "dtype"] {
        let mut wire = basic_reference_wire();
        wire["generator"].as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<Reference>(wire).is_err(),
            "missing generator {field}"
        );
    }
    for (path, value) in [
        ("/checkpointManifest", serde_json::json!("invalid")),
        ("/generator/attention", serde_json::json!("auto")),
        ("/generator/gpu", serde_json::json!("software renderer")),
        ("/generator/dtype", serde_json::json!("bfloat16")),
        ("/generator/cuda", serde_json::Value::Null),
        ("/queries", serde_json::json!([])),
        ("/documents/0/values", serde_json::json!([1.0])),
        ("/documents/0/text", serde_json::json!("")),
    ] {
        let mut wire = basic_reference_wire();
        *wire.pointer_mut(path).unwrap() = value;
        assert!(
            serde_json::from_value::<Reference>(wire).is_err(),
            "invalid {path}"
        );
    }
    let mut unsupported = basic_reference_wire();
    unsupported["space"]["precision"] = "float32".into();
    unsupported["generator"]["dtype"] = "float32".into();
    assert!(serde_json::from_value::<Reference>(unsupported).is_err());
    for path in ["/generator", "/queries/0", "/documents/0"] {
        let mut wire = basic_reference_wire();
        wire.pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), true.into());
        assert!(serde_json::from_value::<Reference>(wire).is_err());
    }
}
