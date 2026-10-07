use super::*;
#[derive(Clone)]
struct FakeMediaProviderState {
    base_url: String,
    http: reqwest::Client,
    completion_delay: Duration,
    webhook_secret: String,
    cancellations: Arc<Mutex<BTreeMap<String, FakeMediaCancellation>>>,
}

struct FakeMediaCancellation {
    stop: tokio::sync::oneshot::Sender<()>,
    accepts_delete: bool,
}

#[derive(Debug, Deserialize)]
struct FakeBillingSearchRequest {
    #[serde(default)]
    prediction_uuids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct FakePredictionDeleteRequest {
    ids: Vec<String>,
}

pub(super) async fn cmd_fake_media_provider(
    port: u16,
    ready_file: Option<PathBuf>,
    completion_delay_ms: u64,
    webhook_secret: String,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let base_url = format!("http://{}", listener.local_addr()?);
    let state = FakeMediaProviderState {
        base_url,
        http: reqwest::Client::new(),
        completion_delay: Duration::from_millis(completion_delay_ms),
        webhook_secret,
        cancellations: Arc::new(Mutex::new(BTreeMap::new())),
    };
    let router = AxumRouter::new()
        .route("/api/v3/models", axum_get(fake_media_models))
        .route("/api/v3/billings/search", axum_post(fake_media_billing))
        .route("/api/v3/predictions/delete", axum_post(fake_media_delete))
        .route("/api/v3/{*model_id}", axum_post(fake_media_submit))
        .route("/outputs/fake.png", axum_get(fake_media_output))
        .with_state(state);
    if let Some(path) = ready_file {
        std::fs::write(path, b"ready\n")?;
    }
    axum::serve(listener, router).await?;
    Ok(())
}

fn fake_media_envelope(data: Value) -> AxumJson<Value> {
    AxumJson(json!({
        "code": 200,
        "message": "ok",
        "data": data,
    }))
}

async fn fake_media_models() -> AxumJson<Value> {
    fake_media_envelope(json!([
        {
            "model_id": "fake/image",
            "name": "Fake image",
            "type": "image-to-image",
            "description": "Deterministic local smoke-test model.",
            "base_price": 0.01,
            "formula": "fixed smoke price",
            "api_schema": {
                "api_schemas": [
                    {
                        "type": "model_run",
                        "request_schema": {
                            "type": "object",
                            "required": ["prompt"],
                            "properties": {
                                "prompt": { "type": "string" }
                            },
                            "additionalProperties": true
                        }
                    }
                ]
            }
        }
    ]))
}

async fn fake_media_submit(
    AxumState(state): AxumState<FakeMediaProviderState>,
    AxumPath(model_id): AxumPath<String>,
    AxumQuery(query): AxumQuery<BTreeMap<String, String>>,
    AxumJson(input): AxumJson<Value>,
) -> AxumJson<Value> {
    let prediction_id = format!("fake-{}", uuid::Uuid::new_v4());
    let output_url = format!("{}/outputs/fake.png", state.base_url);
    if let Some(webhook_url) = query.get("webhook").cloned() {
        let (cancel, cancelled) = tokio::sync::oneshot::channel();
        let accepts_delete = input
            .get("_fake_provider_cancellation")
            .and_then(Value::as_str)
            != Some("not_deleted");
        state
            .cancellations
            .lock()
            .expect("fake cancellation registry available")
            .insert(
                prediction_id.clone(),
                FakeMediaCancellation {
                    stop: cancel,
                    accepts_delete,
                },
            );
        let http = state.http.clone();
        let completion_delay = state.completion_delay;
        let webhook_secret = state.webhook_secret.clone();
        let cancellations = state.cancellations.clone();
        let tracked_prediction_id = prediction_id.clone();
        let terminal = json!({
            "id": prediction_id,
            "model": model_id,
            "outputs": [output_url],
            "status": "completed",
            "input": input,
            "executionTime": 0.2,
        });
        tokio::spawn(async move {
            tokio::select! {
                () = tokio::time::sleep(completion_delay) => {
                    let body = serde_json::to_vec(&terminal).expect("fake webhook serializes");
                    let webhook_id = format!("fake-webhook-{}", uuid::Uuid::now_v7());
                    let timestamp = chrono::Utc::now().timestamp().to_string();
                    let signature = sign_webhook(
                        &webhook_secret,
                        &webhook_id,
                        &timestamp,
                        &body,
                    );
                    if let Err(err) = http
                        .post(webhook_url)
                        .header("content-type", "application/json")
                        .header("webhook-id", webhook_id)
                        .header("webhook-timestamp", timestamp)
                        .header("webhook-signature", signature)
                        .body(body)
                        .send()
                        .await
                    {
                        eprintln!("fake media provider webhook failed: {err}");
                    }
                }
                _ = cancelled => {}
            }
            cancellations
                .lock()
                .expect("fake cancellation registry available")
                .remove(&tracked_prediction_id);
        });
    }

    fake_media_envelope(json!({
        "id": prediction_id,
        "model": model_id,
        "outputs": [],
        "status": "processing",
    }))
}

fn sign_webhook(secret: &str, webhook_id: &str, timestamp: &str, body: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let key = secret.strip_prefix("whsec_").unwrap_or(secret);
    let mut mac = <Hmac<Sha256> as hmac::KeyInit>::new_from_slice(key.as_bytes())
        .expect("HMAC accepts every key length");
    mac.update(webhook_id.as_bytes());
    mac.update(b".");
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    format!("v3,{}", hex::encode(mac.finalize().into_bytes()))
}

async fn fake_media_delete(
    AxumState(state): AxumState<FakeMediaProviderState>,
    AxumJson(request): AxumJson<FakePredictionDeleteRequest>,
) -> AxumJson<Value> {
    let mut deleted_count = 0_u64;
    let mut cancellations = state
        .cancellations
        .lock()
        .expect("fake cancellation registry available");
    for prediction_id in request.ids {
        if let Some(cancellation) = cancellations.remove(&prediction_id) {
            if cancellation.accepts_delete {
                let _ = cancellation.stop.send(());
                deleted_count += 1;
                eprintln!("fake media provider cancellation accepted: {prediction_id}");
            } else {
                cancellations.insert(prediction_id.clone(), cancellation);
                eprintln!("fake media provider cancellation not deleted: {prediction_id}");
            }
        }
    }
    fake_media_envelope(json!({ "deleted_count": deleted_count }))
}

async fn fake_media_billing(
    AxumJson(request): AxumJson<FakeBillingSearchRequest>,
) -> AxumJson<Value> {
    let prediction_id = request
        .prediction_uuids
        .first()
        .cloned()
        .unwrap_or_else(|| "fake-unknown".to_string());
    fake_media_envelope(json!({
        "items": [
            {
                "uuid": format!("billing-{prediction_id}"),
                "billing_type": "deduct",
                "price": 0.01,
                "created_at": Utc::now(),
                "updated_at": Utc::now(),
                "order": {
                    "uuid": format!("order-{prediction_id}"),
                    "state": "completed",
                    "status": "completed"
                },
                "prediction": {
                    "uuid": prediction_id,
                    "model_uuid": "fake/image",
                    "status": "completed"
                }
            }
        ]
    }))
}

async fn fake_media_output() -> impl AxumIntoResponse {
    eprintln!("fake media provider output fetched");
    let bytes = BASE64_STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=")
        .expect("valid embedded PNG");
    ([("content-type", "image/png")], bytes)
}
