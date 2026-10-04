//! Native HTTP contract checks. Synthetic vectors provide no GPU/model qualification.
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use secrecy::SecretString;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Semaphore, mpsc},
    task::JoinHandle,
    time::timeout,
};
use veoveo_embedding_client::*;
use veoveo_embedding_contract::{EmbeddingDimension, EmbeddingModelId, EmbeddingModelRevision};
use veoveo_types::Sha256Digest;

#[derive(Clone)]
struct Runtime {
    mode: Arc<AtomicU8>,
    calls: Arc<AtomicUsize>,
    inputs: Arc<Mutex<Vec<Request>>>,
    seen: mpsc::UnboundedSender<i32>,
    release: Arc<Semaphore>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    model: String,
    input: Vec<String>,
    encoding_format: String,
    use_activation: bool,
    priority: i32,
}
struct Fixture {
    endpoint: EmbeddingEndpoint,
    runtime: Runtime,
    seen: mpsc::UnboundedReceiver<i32>,
    task: JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    async fn new() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint =
            EmbeddingEndpoint::parse(&format!("http://{}", listener.local_addr().unwrap()))
                .unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        let runtime = Runtime {
            mode: Arc::new(AtomicU8::new(0)),
            calls: Arc::new(AtomicUsize::new(0)),
            inputs: Arc::default(),
            seen: tx,
            release: Arc::new(Semaphore::new(0)),
        };
        let app = Router::new()
            .route("/v1/models", get(models))
            .route("/v1/embeddings", post(embed))
            .with_state(runtime.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            endpoint,
            runtime,
            seen: rx,
            task,
        }
    }
    fn config(&self) -> EmbeddingClientConfig {
        EmbeddingClientConfig::new(
            self.endpoint.clone(),
            SecretString::from("fixture-key"),
            space(),
        )
        .with_deadline(Duration::from_secs(3))
        .unwrap()
    }
    async fn next(&mut self) -> i32 {
        timeout(Duration::from_secs(2), self.seen.recv())
            .await
            .unwrap()
            .unwrap()
    }
}
fn space() -> EmbeddingSpace {
    EmbeddingSpace {
        model: EmbeddingModelId::parse("fixture").unwrap(),
        revision: EmbeddingModelRevision::parse("pinned-revision").unwrap(),
        dimension: EmbeddingDimension::new(3).unwrap(),
        runtime_image: Sha256Digest::from_bytes([1; 32]),
    }
}
fn batch(texts: &[&str]) -> EmbeddingBatch {
    EmbeddingBatch::new(
        texts
            .iter()
            .map(|text| EmbeddingText::new(*text).unwrap())
            .collect(),
    )
    .unwrap()
}
async fn models(State(state): State<Runtime>, headers: HeaderMap) -> Json<Value> {
    assert_eq!(headers["authorization"], "Bearer fixture-key");
    let mode = state.mode.load(Ordering::SeqCst);
    let id = if mode == 20 { "wrong" } else { "fixture" };
    let mut data =
        vec![json!({"id":id, "object":"model", "root":"/models/fixture", "max_model_len":32768})];
    if mode == 21 {
        data.push(json!({"id":"fixture", "object":"wrong"}));
    }
    Json(json!({"object":"list", "data":data}))
}
async fn embed(
    State(state): State<Runtime>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    assert_eq!(headers["authorization"], "Bearer fixture-key");
    assert_eq!(request.model, "fixture");
    assert_eq!(request.encoding_format, "float");
    assert!(request.use_activation);
    state.calls.fetch_add(1, Ordering::SeqCst);
    state.seen.send(request.priority).unwrap();
    let mode = state.mode.load(Ordering::SeqCst);
    if mode == 30 && request.priority == 10 {
        state.release.acquire().await.unwrap().forget();
    }
    let mut data: Vec<_> = request.input.iter().enumerate().map(|(index, _)| json!({"index":index, "object":"embedding", "embedding":if index % 2 == 0 { vec![1.,0.,0.] } else { vec![0.,1.,0.] }})).collect();
    state.inputs.lock().unwrap().push(request);
    match mode {
        1 => data.reverse(),
        2 => data[0]["index"] = json!(100),
        3 => data[1]["index"] = json!(0),
        4 => {
            data.pop();
        }
        5 => data[0]["embedding"] = json!([1., 0.]),
        6 => data[0]["embedding"] = json!([0., 0., 0.]),
        7 => data[0]["embedding"] = json!(["NaN", 0., 0.]),
        8 => {
            return (
                StatusCode::TEMPORARY_REDIRECT,
                [("location", "/v1/embeddings")],
            )
                .into_response();
        }
        9 => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "fixture-key and sensitive document text",
            )
                .into_response();
        }
        10 => {
            let chunks = futures::stream::iter(vec![Ok::<_, std::convert::Infallible>(vec![
                    b' ';
                    8 * 1024 * 1024
                        + 1
                ])]);
            return Response::new(Body::from_stream(chunks));
        }
        11 => data[0]["object"] = json!("wrong"),
        _ => {}
    }
    Json(json!({"object":"list", "model":if mode == 12 { "wrong" } else { "fixture" }, "data":data, "usage":{"prompt_tokens":10,"total_tokens":10}})).into_response()
}

#[tokio::test]
async fn preserves_order_space_plain_documents_and_model_card_query_formatting() {
    let fixture = Fixture::new().await;
    fixture.runtime.mode.store(1, Ordering::SeqCst);
    let client = EmbeddingClient::connect(fixture.config()).await.unwrap();
    let vectors = client
        .embed_documents(&batch(&["one", "two"]), EmbeddingPriority::Bulk)
        .await
        .unwrap();
    assert_eq!(vectors[0].values(), &[1., 0., 0.]);
    assert_eq!(vectors[1].values(), &[0., 1., 0.]);
    assert_eq!(vectors[0].space(), &space());
    let task = EmbeddingTask::new("Find related passages").unwrap();
    client
        .embed_query(&task, &EmbeddingText::new("文 query").unwrap())
        .await
        .unwrap();
    client
        .embed_queries(&task, &batch(&["a", "b"]), EmbeddingPriority::Interactive)
        .await
        .unwrap();
    let inputs = fixture.runtime.inputs.lock().unwrap();
    assert_eq!(inputs[0].input, ["one", "two"]);
    assert_eq!(inputs[0].priority, 10);
    assert_eq!(
        inputs[1].input,
        ["Instruct: Find related passages\nQuery:文 query"]
    );
    assert_eq!(inputs[1].priority, 0);
    assert_eq!(inputs[2].input.len(), 2);
}

#[tokio::test]
async fn rejects_wrong_models_malformed_vectors_redirects_and_oversized_streams_without_retry() {
    let fixture = Fixture::new().await;
    for mode in [20, 21] {
        fixture.runtime.mode.store(mode, Ordering::SeqCst);
        assert!(matches!(
            EmbeddingClient::connect(fixture.config()).await,
            Err(EmbeddingClientError::Response(_))
        ));
    }
    fixture.runtime.mode.store(0, Ordering::SeqCst);
    let client = EmbeddingClient::connect(fixture.config()).await.unwrap();
    for mode in 2..=12 {
        fixture.runtime.mode.store(mode, Ordering::SeqCst);
        let before = fixture.runtime.calls.load(Ordering::SeqCst);
        let error = client
            .embed_documents(&batch(&["one", "two"]), EmbeddingPriority::Bulk)
            .await
            .unwrap_err();
        assert_eq!(fixture.runtime.calls.load(Ordering::SeqCst), before + 1);
        assert!(!format!("{error:?}: {error}").contains("fixture-key"));
        assert!(!format!("{error:?}: {error}").contains("sensitive document"));
        if mode == 8 {
            assert!(matches!(error, EmbeddingClientError::HttpStatus(307)));
        }
        if mode == 10 {
            assert!(matches!(
                error,
                EmbeddingClientError::Response("runtime response exceeds its byte limit")
            ));
        }
    }
}

#[tokio::test]
async fn clones_share_bulk_capacity_while_queries_progress_and_cancellation_releases_capacity() {
    let mut fixture = Fixture::new().await;
    fixture.runtime.mode.store(30, Ordering::SeqCst);
    let client = EmbeddingClient::connect(fixture.config().with_concurrency(2, 1).unwrap())
        .await
        .unwrap();
    let spawn_bulk = |client: EmbeddingClient| {
        tokio::spawn(async move {
            client
                .embed_documents(&batch(&["bulk"]), EmbeddingPriority::Bulk)
                .await
        })
    };
    let first = spawn_bulk(client.clone());
    assert_eq!(fixture.next().await, 10);
    let second = spawn_bulk(client.clone());
    let task = EmbeddingTask::new("Retrieve").unwrap();
    client
        .embed_query(&task, &EmbeddingText::new("interactive").unwrap())
        .await
        .unwrap();
    assert_eq!(fixture.next().await, 0);
    assert_eq!(fixture.runtime.calls.load(Ordering::SeqCst), 2);
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    assert_eq!(fixture.next().await, 10);
    fixture.runtime.release.add_permits(2);
    timeout(Duration::from_secs(2), second)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn deadline_covers_capacity_wait_and_transport() {
    let mut fixture = Fixture::new().await;
    fixture.runtime.mode.store(30, Ordering::SeqCst);
    let client = EmbeddingClient::connect(
        fixture
            .config()
            .with_deadline(Duration::from_millis(150))
            .unwrap(),
    )
    .await
    .unwrap();
    let mut pending = tokio::task::JoinSet::new();
    for index in 0..10 {
        let client = client.clone();
        pending.spawn(async move {
            client
                .embed_documents(&batch(&["queued"]), EmbeddingPriority::Bulk)
                .await
        });
        if index == 0 {
            assert_eq!(fixture.next().await, 10);
        }
    }
    // Restarting the timeout after acquiring a permit would serialize ten
    // 150 ms waits. All calls must instead finish within one deadline window.
    timeout(Duration::from_millis(500), async {
        while let Some(result) = pending.join_next().await {
            assert!(matches!(
                result.unwrap(),
                Err(EmbeddingClientError::Deadline)
            ));
        }
    })
    .await
    .unwrap();
    fixture.runtime.release.add_permits(10);
    client
        .embed_query(
            &EmbeddingTask::new("Find").unwrap(),
            &EmbeddingText::new("now").unwrap(),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn rejects_invalid_configuration_and_formatted_oversize_before_dispatch() {
    for value in [
        "ftp://host",
        "http://user:secret@host",
        "http://host/path",
        "http://host?q=secret",
        "http://host/#fragment",
    ] {
        assert!(EmbeddingEndpoint::parse(value).is_err());
    }
    let fixture = Fixture::new().await;
    for (total, bulk) in [(0, 0), (1, 1), (4, 0), (4, 4), (65, 1)] {
        assert!(fixture.config().with_concurrency(total, bulk).is_err());
    }
    assert!(fixture.config().with_deadline(Duration::ZERO).is_err());
    assert!(
        fixture
            .config()
            .with_deadline(Duration::from_secs(121))
            .is_err()
    );
    let bad_key = EmbeddingClientConfig::new(
        fixture.endpoint.clone(),
        SecretString::from("bad\nkey"),
        space(),
    );
    assert!(matches!(
        EmbeddingClient::connect(bad_key).await,
        Err(EmbeddingClientError::Configuration(_))
    ));
    let client = EmbeddingClient::connect(fixture.config()).await.unwrap();
    let batch =
        EmbeddingBatch::new(vec![EmbeddingText::new("a".repeat(16384)).unwrap(); 8]).unwrap();
    assert!(matches!(
        client
            .embed_queries(
                &EmbeddingTask::new("Retrieve").unwrap(),
                &batch,
                EmbeddingPriority::Bulk
            )
            .await,
        Err(EmbeddingClientError::Input(_))
    ));
    assert_eq!(fixture.runtime.calls.load(Ordering::SeqCst), 0);
}
