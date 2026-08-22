//! Cluster-internal protobuf HTTP surface for Recording Hub ingest.

use std::{future::Future, time::Duration};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use prost::Message;
use veoveo_mcp_contract::{GatewayInternalResourceIdentity, GatewayInternalResourceTokenVerifier};
use veoveo_platform_store::{
    RecordingIngestQuota as StoreRecordingIngestQuota, RecordingIngestStreamId, StoreError,
};
use veoveo_recording_protocol::{
    BatchValidationError, MEDIA_TYPE,
    v1::{
        AuthorizedFinishRecordingStreamRequest, AuthorizedOpenRecordingStreamRequest,
        AuthorizedRecordingBatchRequest, AuthorizedRecordingBlueprintRequest,
        AuthorizedRecordingProducer, FinishRecordingStreamResult, IngestError, IngestErrorCode,
        RecordingIngestQuota as ProtocolRecordingIngestQuota, RecordingStreamFinishMode,
    },
};

use crate::RecordingIngestService;

const INTERNAL_STREAMS_PATH: &str = "/internal/recording-ingest/v1/streams";
const INTERNAL_DIAGNOSTICS_PATH: &str = "/internal/recording-ingest/v1/diagnostics";
const STORAGE_OPERATION_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone)]
struct IngestHttpState {
    service: RecordingIngestService,
    verifier: GatewayInternalResourceTokenVerifier,
}

struct HttpFailure(Box<Response>);

impl From<Response> for HttpFailure {
    fn from(response: Response) -> Self {
        Self(Box::new(response))
    }
}

impl IntoResponse for HttpFailure {
    fn into_response(self) -> Response {
        *self.0
    }
}

pub fn recording_ingest_internal_router(
    service: RecordingIngestService,
    verifier: GatewayInternalResourceTokenVerifier,
    maximum_batch_bytes: u64,
) -> Router {
    let maximum_body_bytes = usize::try_from(maximum_batch_bytes)
        .unwrap_or(usize::MAX)
        .saturating_add(64 * 1024);
    Router::new()
        .route("/healthz", get(health))
        .route("/readyz", get(health))
        .route(INTERNAL_DIAGNOSTICS_PATH, get(ingest_diagnostics))
        .route(INTERNAL_STREAMS_PATH, post(open_stream))
        .route(
            &format!("{INTERNAL_STREAMS_PATH}/{{stream_id}}/status"),
            post(stream_status),
        )
        .route(
            &format!("{INTERNAL_STREAMS_PATH}/{{stream_id}}/batches/{{sequence}}"),
            put(append_batch),
        )
        .route(
            &format!("{INTERNAL_STREAMS_PATH}/{{stream_id}}/blueprints/{{revision}}"),
            put(publish_blueprint),
        )
        .route(
            &format!("{INTERNAL_STREAMS_PATH}/{{stream_id}}/finish"),
            post(finish_stream),
        )
        .layer(DefaultBodyLimit::max(maximum_body_bytes))
        .with_state(IngestHttpState { service, verifier })
}

async fn health(State(state): State<IngestHttpState>) -> Response {
    match bounded_service_operation("healthcheck", state.service.healthcheck()).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

async fn ingest_diagnostics(State(state): State<IngestHttpState>, headers: HeaderMap) -> Response {
    if let Err(error) = authenticate(&state, &headers) {
        return error.into_response();
    }
    Json(state.service.diagnostics()).into_response()
}

async fn open_stream(
    State(state): State<IngestHttpState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let gateway = match authenticate(&state, &headers) {
        Ok(identity) => identity,
        Err(error) => return error.into_response(),
    };
    let envelope = match decode::<AuthorizedOpenRecordingStreamRequest>(&headers, &body) {
        Ok(envelope) => envelope,
        Err(error) => return error.into_response(),
    };
    let Some(producer) = envelope.producer.as_ref() else {
        return ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRequest,
            "producer authorization is required",
            None,
        );
    };
    let Some(request) = envelope.request.as_ref() else {
        return ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRequest,
            "open stream request is required",
            None,
        );
    };
    let service = state.service.clone();
    let producer = producer.clone();
    let request = request.clone();
    match bounded_mutation_operation("open stream", async move {
        service
            .open(
                &gateway,
                &producer,
                &request.source_stream_id,
                &request.application_id,
                &request.recording_id,
            )
            .await
    })
    .await
    {
        Ok(stream) => protobuf_response(StatusCode::OK, &stream),
        Err(response) => response,
    }
}

async fn stream_status(
    State(state): State<IngestHttpState>,
    Path(stream_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let gateway = match authenticate(&state, &headers) {
        Ok(identity) => identity,
        Err(error) => return error.into_response(),
    };
    let producer = match decode::<AuthorizedRecordingProducer>(&headers, &body) {
        Ok(producer) => producer,
        Err(error) => return error.into_response(),
    };
    let stream_id = match parse_stream_id(&stream_id) {
        Ok(stream_id) => stream_id,
        Err(error) => return error.into_response(),
    };
    match bounded_service_operation(
        "read stream status",
        state.service.status(&gateway, &producer, stream_id),
    )
    .await
    {
        Ok(stream) => protobuf_response(StatusCode::OK, &stream),
        Err(response) => response,
    }
}

async fn append_batch(
    State(state): State<IngestHttpState>,
    Path((stream_id, sequence)): Path<(String, u64)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let gateway = match authenticate(&state, &headers) {
        Ok(identity) => identity,
        Err(error) => return error.into_response(),
    };
    let envelope = match decode::<AuthorizedRecordingBatchRequest>(&headers, &body) {
        Ok(envelope) => envelope,
        Err(error) => return error.into_response(),
    };
    let Some(producer) = envelope.producer.as_ref() else {
        return ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRequest,
            "producer authorization is required",
            None,
        );
    };
    let Some(batch) = envelope.batch.as_ref() else {
        return ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRequest,
            "recording batch is required",
            None,
        );
    };
    if batch.sequence != sequence {
        return ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRequest,
            "path and batch sequences differ",
            None,
        );
    }
    let stream_id = match parse_stream_id(&stream_id) {
        Ok(stream_id) => stream_id,
        Err(error) => return error.into_response(),
    };
    let service = state.service.clone();
    let producer = producer.clone();
    let batch = batch.clone();
    match bounded_mutation_operation("append batch", async move {
        service.append(&gateway, &producer, stream_id, &batch).await
    })
    .await
    {
        Ok(result) => protobuf_response(StatusCode::OK, &result),
        Err(response) => response,
    }
}

async fn publish_blueprint(
    State(state): State<IngestHttpState>,
    Path((stream_id, revision)): Path<(String, u64)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let gateway = match authenticate(&state, &headers) {
        Ok(identity) => identity,
        Err(error) => return error.into_response(),
    };
    let envelope = match decode::<AuthorizedRecordingBlueprintRequest>(&headers, &body) {
        Ok(envelope) => envelope,
        Err(error) => return error.into_response(),
    };
    let Some(producer) = envelope.producer.as_ref() else {
        return ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRequest,
            "producer authorization is required",
            None,
        );
    };
    let Some(blueprint) = envelope.blueprint.as_ref() else {
        return ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRequest,
            "recording Blueprint publication is required",
            None,
        );
    };
    if blueprint.revision != revision {
        return ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRequest,
            "path and Blueprint revisions differ",
            None,
        );
    }
    let stream_id = match parse_stream_id(&stream_id) {
        Ok(stream_id) => stream_id,
        Err(error) => return error.into_response(),
    };
    let service = state.service.clone();
    let producer = producer.clone();
    let blueprint = blueprint.clone();
    match bounded_mutation_operation("publish Blueprint", async move {
        service
            .publish_blueprint(&gateway, &producer, stream_id, &blueprint)
            .await
    })
    .await
    {
        Ok(result) => protobuf_response(StatusCode::OK, &result),
        Err(response) => response,
    }
}

async fn finish_stream(
    State(state): State<IngestHttpState>,
    Path(stream_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let gateway = match authenticate(&state, &headers) {
        Ok(identity) => identity,
        Err(error) => return error.into_response(),
    };
    let envelope = match decode::<AuthorizedFinishRecordingStreamRequest>(&headers, &body) {
        Ok(envelope) => envelope,
        Err(error) => return error.into_response(),
    };
    let Some(producer) = envelope.producer.as_ref() else {
        return ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRequest,
            "producer authorization is required",
            None,
        );
    };
    let Some(request) = envelope.request.as_ref() else {
        return ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRequest,
            "finish request is required",
            None,
        );
    };
    let mode = match RecordingStreamFinishMode::try_from(request.mode) {
        Ok(RecordingStreamFinishMode::ContinueRecording) => {
            RecordingStreamFinishMode::ContinueRecording
        }
        Ok(RecordingStreamFinishMode::CompleteRecording) => {
            RecordingStreamFinishMode::CompleteRecording
        }
        _ => {
            return ingest_error(
                StatusCode::BAD_REQUEST,
                IngestErrorCode::InvalidRequest,
                "recording stream finish mode is required",
                None,
            );
        }
    };
    let stream_id = match parse_stream_id(&stream_id) {
        Ok(stream_id) => stream_id,
        Err(error) => return error.into_response(),
    };
    let service = state.service.clone();
    let producer = producer.clone();
    match bounded_mutation_operation("finish stream", async move {
        service.finish(&gateway, &producer, stream_id, mode).await
    })
    .await
    {
        Ok(stream) => protobuf_response(
            StatusCode::OK,
            &FinishRecordingStreamResult {
                stream: Some(stream),
            },
        ),
        Err(response) => response,
    }
}

async fn bounded_service_operation<T>(
    operation: &'static str,
    future: impl Future<Output = anyhow::Result<T>>,
) -> Result<T, Response> {
    bounded_service_operation_with_timeout(operation, STORAGE_OPERATION_TIMEOUT, future).await
}

async fn bounded_mutation_operation<T, F>(operation: &'static str, future: F) -> Result<T, Response>
where
    T: Send + 'static,
    F: Future<Output = anyhow::Result<T>> + Send + 'static,
{
    bounded_mutation_operation_with_timeout(operation, STORAGE_OPERATION_TIMEOUT, future).await
}

async fn bounded_mutation_operation_with_timeout<T, F>(
    operation: &'static str,
    timeout: Duration,
    future: F,
) -> Result<T, Response>
where
    T: Send + 'static,
    F: Future<Output = anyhow::Result<T>> + Send + 'static,
{
    let mut task = tokio::spawn(future);
    match tokio::time::timeout(timeout, &mut task).await {
        Ok(Ok(Ok(value))) => Ok(value),
        Ok(Ok(Err(error))) => Err(service_error(error)),
        Ok(Err(error)) => {
            tracing::warn!(operation, error = %error, "recording ingest mutation task failed");
            Err(storage_unavailable_error(
                "recording ingest storage operation failed",
            ))
        }
        Err(_) => {
            tracing::warn!(
                operation,
                timeout_milliseconds = timeout.as_millis(),
                "recording ingest response timed out; mutation continues"
            );
            // Dropping a Tokio JoinHandle detaches the task. The mutation keeps the
            // service's materialization lock until its filesystem and catalog state
            // are coherent, while an idempotent producer retry receives a bounded
            // response and queues behind it.
            drop(task);
            Err(storage_unavailable_error(
                "recording ingest storage operation timed out",
            ))
        }
    }
}

async fn bounded_service_operation_with_timeout<T>(
    operation: &'static str,
    timeout: Duration,
    future: impl Future<Output = anyhow::Result<T>>,
) -> Result<T, Response> {
    match tokio::time::timeout(timeout, future).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(service_error(error)),
        Err(_) => {
            tracing::warn!(
                operation,
                timeout_milliseconds = timeout.as_millis(),
                "recording ingest storage operation timed out"
            );
            Err(storage_unavailable_error(
                "recording ingest storage operation timed out",
            ))
        }
    }
}

fn storage_unavailable_error(message: &str) -> Response {
    ingest_error(
        StatusCode::SERVICE_UNAVAILABLE,
        IngestErrorCode::StorageUnavailable,
        message,
        None,
    )
}

fn authenticate(
    state: &IngestHttpState,
    headers: &HeaderMap,
) -> Result<GatewayInternalResourceIdentity, HttpFailure> {
    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            HttpFailure::from(ingest_error(
                StatusCode::UNAUTHORIZED,
                IngestErrorCode::Unauthorized,
                "gateway bearer token is required",
                None,
            ))
        })?;
    state.verifier.verify(authorization).map_err(|_| {
        HttpFailure::from(ingest_error(
            StatusCode::UNAUTHORIZED,
            IngestErrorCode::Unauthorized,
            "gateway bearer token is invalid",
            None,
        ))
    })
}

fn decode<T: Message + Default>(headers: &HeaderMap, body: &[u8]) -> Result<T, HttpFailure> {
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        != Some(MEDIA_TYPE)
    {
        return Err(ingest_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            IngestErrorCode::InvalidRequest,
            "canonical recording ingest media type is required",
            None,
        )
        .into());
    }
    T::decode(body).map_err(|_| {
        HttpFailure::from(ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRequest,
            "protobuf request is invalid",
            None,
        ))
    })
}

fn parse_stream_id(value: &str) -> Result<RecordingIngestStreamId, HttpFailure> {
    let id = value.parse::<RecordingIngestStreamId>().map_err(|_| {
        HttpFailure::from(ingest_error(
            StatusCode::NOT_FOUND,
            IngestErrorCode::StreamNotFound,
            "recording ingest stream was not found",
            None,
        ))
    })?;
    if id.as_uuid().get_version_num() != 7 {
        return Err(ingest_error(
            StatusCode::NOT_FOUND,
            IngestErrorCode::StreamNotFound,
            "recording ingest stream was not found",
            None,
        )
        .into());
    }
    Ok(id)
}

fn service_error(error: anyhow::Error) -> Response {
    if let Some(validation) = error.downcast_ref::<crate::RecordingBlueprintPublicationError>() {
        return match validation {
            crate::RecordingBlueprintPublicationError::NotAllowed => ingest_error(
                StatusCode::FORBIDDEN,
                IngestErrorCode::BlueprintNotAllowed,
                &validation.to_string(),
                None,
            ),
            crate::RecordingBlueprintPublicationError::AssociationMismatch => ingest_error(
                StatusCode::FORBIDDEN,
                IngestErrorCode::BlueprintAssociationMismatch,
                &validation.to_string(),
                None,
            ),
            crate::RecordingBlueprintPublicationError::Invalid(_) => ingest_error(
                StatusCode::BAD_REQUEST,
                IngestErrorCode::InvalidBlueprint,
                &validation.to_string(),
                None,
            ),
        };
    }
    if let Some(validation) = error.downcast_ref::<BatchValidationError>() {
        return match validation {
            BatchValidationError::PayloadTooLarge { .. } => ingest_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                IngestErrorCode::PayloadTooLarge,
                &validation.to_string(),
                None,
            ),
            BatchValidationError::UnsupportedPayloadFormat => ingest_error(
                StatusCode::BAD_REQUEST,
                IngestErrorCode::UnsupportedPayload,
                &validation.to_string(),
                None,
            ),
            _ => ingest_error(
                StatusCode::BAD_REQUEST,
                IngestErrorCode::InvalidRequest,
                &validation.to_string(),
                None,
            ),
        };
    }
    if let Some(store) = error.downcast_ref::<StoreError>() {
        return match store {
            StoreError::RecordingIngestStreamNotFound(_) => ingest_error(
                StatusCode::NOT_FOUND,
                IngestErrorCode::StreamNotFound,
                "recording ingest stream was not found",
                None,
            ),
            StoreError::RecordingIngestStreamStateConflict { .. }
            | StoreError::RecordingIngestStreamExpired(_) => ingest_error(
                StatusCode::CONFLICT,
                IngestErrorCode::StreamFinished,
                &store.to_string(),
                None,
            ),
            StoreError::RecordingIngestSequenceGap { expected, .. } => ingest_error(
                StatusCode::CONFLICT,
                IngestErrorCode::SequenceGap,
                &store.to_string(),
                Some(*expected),
            ),
            StoreError::RecordingIngestDigestConflict { .. } => ingest_error(
                StatusCode::CONFLICT,
                IngestErrorCode::DigestConflict,
                &store.to_string(),
                None,
            ),
            StoreError::RecordingBlueprintRevisionConflict { .. }
            | StoreError::RecordingBlueprintRevisionGap { .. } => ingest_error(
                StatusCode::CONFLICT,
                IngestErrorCode::BlueprintRevisionConflict,
                &store.to_string(),
                None,
            ),
            StoreError::RecordingIngestQuotaExceeded { quota } => {
                ingest_quota_error(*quota, &store.to_string())
            }
            StoreError::InvalidRecordingIngestField { .. } => ingest_error(
                StatusCode::BAD_REQUEST,
                IngestErrorCode::InvalidRequest,
                &store.to_string(),
                None,
            ),
            _ => {
                tracing::warn!(error = %store, "recording ingest store operation failed");
                ingest_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    IngestErrorCode::StorageUnavailable,
                    "recording ingest storage is unavailable",
                    None,
                )
            }
        };
    }
    let message = error.to_string();
    if message.contains("quota") || message.contains("byte limit") {
        return ingest_error(
            StatusCode::TOO_MANY_REQUESTS,
            IngestErrorCode::QuotaExceeded,
            "recording ingest quota exceeded",
            None,
        );
    }
    if message.contains("RRD") || message.contains("Rerun") {
        return ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRerunData,
            "recording batch contains invalid Rerun data",
            None,
        );
    }
    tracing::warn!(
        error_code = "untyped_ingest_failure",
        error = ?error,
        "recording ingest request failed without a typed protocol error"
    );
    ingest_error(
        StatusCode::FORBIDDEN,
        IngestErrorCode::Forbidden,
        "recording ingest request is forbidden",
        None,
    )
}

fn protobuf_response(status: StatusCode, message: &impl Message) -> Response {
    let mut response = (status, message.encode_to_vec()).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(MEDIA_TYPE));
    response
}

fn ingest_error(
    status: StatusCode,
    code: IngestErrorCode,
    message: &str,
    expected_sequence: Option<u64>,
) -> Response {
    protobuf_response(
        status,
        &IngestError {
            code: code.into(),
            message: message.to_owned(),
            expected_sequence,
            retry_after_seconds: None,
            quota: None,
        },
    )
}

fn ingest_quota_error(quota: StoreRecordingIngestQuota, message: &str) -> Response {
    let (quota, retry_after_seconds) = quota_response(quota);
    protobuf_response(
        StatusCode::TOO_MANY_REQUESTS,
        &IngestError {
            code: IngestErrorCode::QuotaExceeded.into(),
            message: message.to_owned(),
            expected_sequence: None,
            retry_after_seconds,
            quota: Some(quota.into()),
        },
    )
}

fn quota_response(quota: StoreRecordingIngestQuota) -> (ProtocolRecordingIngestQuota, Option<u64>) {
    match quota {
        StoreRecordingIngestQuota::MaximumStreamBytes => {
            (ProtocolRecordingIngestQuota::MaximumStreamBytes, None)
        }
        StoreRecordingIngestQuota::MaximumConcurrentStreams => (
            ProtocolRecordingIngestQuota::MaximumConcurrentStreams,
            Some(30),
        ),
        StoreRecordingIngestQuota::MaximumBatchesPerMinute => (
            ProtocolRecordingIngestQuota::MaximumBatchesPerMinute,
            Some(1),
        ),
        StoreRecordingIngestQuota::MaximumBytesPerDay => {
            (ProtocolRecordingIngestQuota::MaximumBytesPerDay, Some(60))
        }
        StoreRecordingIngestQuota::MaximumBlueprintBytes => {
            (ProtocolRecordingIngestQuota::MaximumBlueprintBytes, None)
        }
        StoreRecordingIngestQuota::MaximumBlueprintMessages => {
            (ProtocolRecordingIngestQuota::MaximumBlueprintMessages, None)
        }
        StoreRecordingIngestQuota::MaximumBlueprintRevisions => (
            ProtocolRecordingIngestQuota::MaximumBlueprintRevisions,
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_responses_distinguish_rollover_from_retry() {
        assert_eq!(
            quota_response(StoreRecordingIngestQuota::MaximumStreamBytes),
            (ProtocolRecordingIngestQuota::MaximumStreamBytes, None)
        );
        assert_eq!(
            quota_response(StoreRecordingIngestQuota::MaximumBytesPerDay),
            (ProtocolRecordingIngestQuota::MaximumBytesPerDay, Some(60))
        );
    }

    #[tokio::test]
    async fn storage_operation_timeout_returns_retryable_unavailable() {
        let result = bounded_service_operation_with_timeout(
            "test operation",
            Duration::from_millis(1),
            std::future::pending::<anyhow::Result<()>>(),
        )
        .await;
        let response = match result {
            Ok(()) => panic!("pending storage operation unexpectedly completed"),
            Err(response) => response,
        };
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static(MEDIA_TYPE))
        );
    }

    #[tokio::test]
    async fn mutation_response_timeout_does_not_cancel_the_operation() {
        let (completed_tx, completed_rx) = tokio::sync::oneshot::channel();
        let result = bounded_mutation_operation_with_timeout(
            "test mutation",
            Duration::from_millis(1),
            async move {
                tokio::time::sleep(Duration::from_millis(25)).await;
                completed_tx.send(()).unwrap();
                anyhow::Ok(())
            },
        )
        .await;
        let response = match result {
            Ok(()) => panic!("slow mutation unexpectedly met its response deadline"),
            Err(response) => response,
        };

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        tokio::time::timeout(Duration::from_secs(1), completed_rx)
            .await
            .expect("detached mutation did not finish")
            .expect("detached mutation was cancelled");
    }
}
