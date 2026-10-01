//! Installed GPU acceptance. The caller starts the pinned, CUDA-refusing runtime.
use serde::Deserialize;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::time::timeout;
use veoveo_embedding_client::*;

#[derive(Deserialize)]
struct Reference {
    space: EmbeddingSpace,
    task: EmbeddingTask,
    queries: Vec<Sample>,
    documents: Vec<Sample>,
}
#[derive(Deserialize)]
struct Sample {
    text: EmbeddingText,
    values: Vec<f32>,
}

async fn fixture() -> (EmbeddingClient, Reference) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../verification/reference.json");
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
    let client = EmbeddingClient::connect(EmbeddingClientConfig::new(
        endpoint,
        key,
        reference.space.clone(),
    ))
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
