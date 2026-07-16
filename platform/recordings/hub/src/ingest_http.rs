//! Cluster-internal protobuf HTTP surface for Recording Hub ingest.

use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{post, put},
};
use prost::Message;
use veoveo_mcp_contract::{GatewayInternalResourceIdentity, GatewayInternalResourceTokenVerifier};
use veoveo_platform_store::{RecordingIngestStreamId, StoreError};
use veoveo_recording_protocol::{
    BatchValidationError, MEDIA_TYPE,
    v1::{
        AuthorizedFinishRecordingStreamRequest, AuthorizedOpenRecordingStreamRequest,
        AuthorizedRecordingBatchRequest, AuthorizedRecordingProducer, FinishRecordingStreamResult,
        IngestError, IngestErrorCode,
    },
};

use crate::RecordingIngestService;

const INTERNAL_STREAMS_PATH: &str = "/internal/recording-ingest/v1/streams";

#[derive(Clone)]
struct IngestHttpState {
    service: RecordingIngestService,
    verifier: GatewayInternalResourceTokenVerifier,
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
            &format!("{INTERNAL_STREAMS_PATH}/{{stream_id}}/finish"),
            post(finish_stream),
        )
        .layer(DefaultBodyLimit::max(maximum_body_bytes))
        .with_state(IngestHttpState { service, verifier })
}

async fn open_stream(
    State(state): State<IngestHttpState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let gateway = match authenticate(&state, &headers) {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let envelope = match decode::<AuthorizedOpenRecordingStreamRequest>(&headers, &body) {
        Ok(envelope) => envelope,
        Err(response) => return response,
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
    match state
        .service
        .open(
            &gateway,
            producer,
            &request.source_stream_id,
            &request.application_id,
            &request.recording_id,
        )
        .await
    {
        Ok(stream) => protobuf_response(StatusCode::OK, &stream),
        Err(error) => service_error(error),
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
        Err(response) => return response,
    };
    let producer = match decode::<AuthorizedRecordingProducer>(&headers, &body) {
        Ok(producer) => producer,
        Err(response) => return response,
    };
    let stream_id = match parse_stream_id(&stream_id) {
        Ok(stream_id) => stream_id,
        Err(response) => return response,
    };
    match state.service.status(&gateway, &producer, stream_id).await {
        Ok(stream) => protobuf_response(StatusCode::OK, &stream),
        Err(error) => service_error(error),
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
        Err(response) => return response,
    };
    let envelope = match decode::<AuthorizedRecordingBatchRequest>(&headers, &body) {
        Ok(envelope) => envelope,
        Err(response) => return response,
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
        Err(response) => return response,
    };
    match state
        .service
        .append(&gateway, producer, stream_id, batch)
        .await
    {
        Ok(result) => protobuf_response(StatusCode::OK, &result),
        Err(error) => service_error(error),
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
        Err(response) => return response,
    };
    let envelope = match decode::<AuthorizedFinishRecordingStreamRequest>(&headers, &body) {
        Ok(envelope) => envelope,
        Err(response) => return response,
    };
    let Some(producer) = envelope.producer.as_ref() else {
        return ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRequest,
            "producer authorization is required",
            None,
        );
    };
    let stream_id = match parse_stream_id(&stream_id) {
        Ok(stream_id) => stream_id,
        Err(response) => return response,
    };
    match state.service.finish(&gateway, producer, stream_id).await {
        Ok(stream) => protobuf_response(
            StatusCode::OK,
            &FinishRecordingStreamResult {
                stream: Some(stream),
            },
        ),
        Err(error) => service_error(error),
    }
}

fn authenticate(
    state: &IngestHttpState,
    headers: &HeaderMap,
) -> Result<GatewayInternalResourceIdentity, Response> {
    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ingest_error(
                StatusCode::UNAUTHORIZED,
                IngestErrorCode::Unauthorized,
                "gateway bearer token is required",
                None,
            )
        })?;
    state.verifier.verify(authorization).map_err(|_| {
        ingest_error(
            StatusCode::UNAUTHORIZED,
            IngestErrorCode::Unauthorized,
            "gateway bearer token is invalid",
            None,
        )
    })
}

fn decode<T: Message + Default>(headers: &HeaderMap, body: &[u8]) -> Result<T, Response> {
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
        ));
    }
    T::decode(body).map_err(|_| {
        ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRequest,
            "protobuf request is invalid",
            None,
        )
    })
}

fn parse_stream_id(value: &str) -> Result<RecordingIngestStreamId, Response> {
    let id = value.parse::<RecordingIngestStreamId>().map_err(|_| {
        ingest_error(
            StatusCode::NOT_FOUND,
            IngestErrorCode::StreamNotFound,
            "recording ingest stream was not found",
            None,
        )
    })?;
    if id.as_uuid().get_version_num() != 7 {
        return Err(ingest_error(
            StatusCode::NOT_FOUND,
            IngestErrorCode::StreamNotFound,
            "recording ingest stream was not found",
            None,
        ));
    }
    Ok(id)
}

fn service_error(error: anyhow::Error) -> Response {
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
            StoreError::RecordingIngestQuotaExceeded { .. } => ingest_error(
                StatusCode::TOO_MANY_REQUESTS,
                IngestErrorCode::QuotaExceeded,
                &store.to_string(),
                None,
            ),
            StoreError::InvalidRecordingIngestField { .. } => ingest_error(
                StatusCode::BAD_REQUEST,
                IngestErrorCode::InvalidRequest,
                &store.to_string(),
                None,
            ),
            _ => ingest_error(
                StatusCode::SERVICE_UNAVAILABLE,
                IngestErrorCode::StorageUnavailable,
                "recording ingest storage is unavailable",
                None,
            ),
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
        },
    )
}
