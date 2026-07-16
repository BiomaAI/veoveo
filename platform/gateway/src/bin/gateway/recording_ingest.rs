//! Authenticated public recording ingest and discovery routes.

use std::collections::BTreeMap;
use std::time::Instant;

use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use chrono::{TimeDelta, Utc};
use prost::Message;
use veoveo_mcp_contract::{
    AuthOutcome, AuthReasonCode, GatewayAction, PolicyEffect, PrincipalKind,
    RecordingIngestResource, RecordingIngestStreamId, RecordingProducerRegistration, ServerSlug,
    TraceId,
};
use veoveo_mcp_gateway::{
    AuthenticatedSubject, BearerToken, JwtAuthConfig, JwtVerifier, RecordingIngestPolicyRequest,
};
use veoveo_recording_protocol::{
    DISCOVERY_PATH, MEDIA_TYPE, PROTOCOL_VERSION, REQUIRED_SCOPE, STREAMS_PATH,
    v1::{
        AuthorizedFinishRecordingStreamRequest, AuthorizedOpenRecordingStreamRequest,
        AuthorizedRecordingBatchRequest, AuthorizedRecordingProducer, FinishRecordingStreamRequest,
        IngestError, IngestErrorCode, OpenRecordingStreamRequest, RecordingBatch,
        RecordingIngestDiscovery, RerunPayloadFormat,
    },
};

use crate::{
    audit::{AuthAuditTarget, auth_audit_error_response, record_resource_auth_audit},
    http_util::{allowed_gateway_jwt_algorithms, load_jwks},
    runtime::{RecordingIngestGatewayState, current_catalog, current_http_client},
};

const INTERNAL_STREAMS_PATH: &str = "/internal/recording-ingest/v1/streams";
const INTERNAL_TOKEN_TTL_SECONDS: i64 = 60;

pub(super) fn recording_ingest_router(state: RecordingIngestGatewayState) -> Router {
    Router::new()
        .route(DISCOVERY_PATH, get(discovery))
        .route(STREAMS_PATH, post(open_stream))
        .route(&format!("{STREAMS_PATH}/{{stream_id}}"), get(stream_status))
        .route(
            &format!("{STREAMS_PATH}/{{stream_id}}/batches/{{sequence}}"),
            put(append_batch),
        )
        .route(
            &format!("{STREAMS_PATH}/{{stream_id}}/finish"),
            post(finish_stream),
        )
        .layer(DefaultBodyLimit::max(
            usize::try_from(veoveo_recording_protocol::DEFAULT_MAXIMUM_BATCH_BYTES)
                .unwrap_or(usize::MAX)
                .saturating_add(64 * 1024),
        ))
        .with_state(state)
}

async fn discovery(State(state): State<RecordingIngestGatewayState>) -> Response {
    let catalog = current_catalog(&state.catalog);
    let Some(resource) = catalog.single_recording_ingest_resource() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(authorization_server) = catalog.authorization_server(&resource.authorization_server)
    else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    protobuf_response(
        StatusCode::OK,
        &RecordingIngestDiscovery {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            protected_resource: resource.protected_resource.to_string(),
            authorization_server: authorization_server.issuer.to_string(),
            required_scope: REQUIRED_SCOPE.to_owned(),
            streams_endpoint: format!(
                "{}{}",
                state.public_base_url.trim_end_matches('/'),
                STREAMS_PATH
            ),
            maximum_batch_bytes: resource.maximum_batch_bytes,
            payload_formats: vec![RerunPayloadFormat::Rrd0341.into()],
        },
    )
}

async fn open_stream(
    State(state): State<RecordingIngestGatewayState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request = match decode::<OpenRecordingStreamRequest>(&headers, &body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    proxy_authorized(
        &state,
        &headers,
        GatewayAction::RecordingStreamOpen,
        None,
        format!("{INTERNAL_STREAMS_PATH}"),
        AuthorizedOpenRecordingStreamRequest {
            producer: None,
            request: Some(request),
        },
    )
    .await
}

async fn stream_status(
    State(state): State<RecordingIngestGatewayState>,
    Path(stream_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let stream_id = match RecordingIngestStreamId::new(&stream_id) {
        Ok(stream_id) => stream_id,
        Err(_) => return stream_not_found(),
    };
    proxy_authorized(
        &state,
        &headers,
        GatewayAction::RecordingStreamStatus,
        Some(&stream_id),
        format!("{INTERNAL_STREAMS_PATH}/{stream_id}/status"),
        AuthorizedRecordingProducer::default(),
    )
    .await
}

async fn append_batch(
    State(state): State<RecordingIngestGatewayState>,
    Path((stream_id, sequence)): Path<(String, u64)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let stream_id = match RecordingIngestStreamId::new(&stream_id) {
        Ok(stream_id) => stream_id,
        Err(_) => return stream_not_found(),
    };
    let batch = match decode::<RecordingBatch>(&headers, &body) {
        Ok(batch) => batch,
        Err(response) => return response,
    };
    if batch.sequence != sequence {
        return ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRequest,
            "path and batch sequences differ",
        );
    }
    proxy_authorized(
        &state,
        &headers,
        GatewayAction::RecordingBatchAppend,
        Some(&stream_id),
        format!("{INTERNAL_STREAMS_PATH}/{stream_id}/batches/{sequence}"),
        AuthorizedRecordingBatchRequest {
            producer: None,
            batch: Some(batch),
        },
    )
    .await
}

async fn finish_stream(
    State(state): State<RecordingIngestGatewayState>,
    Path(stream_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let stream_id = match RecordingIngestStreamId::new(&stream_id) {
        Ok(stream_id) => stream_id,
        Err(_) => return stream_not_found(),
    };
    if let Err(response) = decode::<FinishRecordingStreamRequest>(&headers, &body) {
        return response;
    }
    proxy_authorized(
        &state,
        &headers,
        GatewayAction::RecordingStreamFinish,
        Some(&stream_id),
        format!("{INTERNAL_STREAMS_PATH}/{stream_id}/finish"),
        AuthorizedFinishRecordingStreamRequest { producer: None },
    )
    .await
}

trait AuthorizedEnvelope: Message + Default {
    fn set_producer(&mut self, producer: AuthorizedRecordingProducer);
}

impl AuthorizedEnvelope for AuthorizedOpenRecordingStreamRequest {
    fn set_producer(&mut self, producer: AuthorizedRecordingProducer) {
        self.producer = Some(producer);
    }
}

impl AuthorizedEnvelope for AuthorizedRecordingBatchRequest {
    fn set_producer(&mut self, producer: AuthorizedRecordingProducer) {
        self.producer = Some(producer);
    }
}

impl AuthorizedEnvelope for AuthorizedFinishRecordingStreamRequest {
    fn set_producer(&mut self, producer: AuthorizedRecordingProducer) {
        self.producer = Some(producer);
    }
}

impl AuthorizedEnvelope for AuthorizedRecordingProducer {
    fn set_producer(&mut self, producer: AuthorizedRecordingProducer) {
        *self = producer;
    }
}

async fn proxy_authorized(
    state: &RecordingIngestGatewayState,
    headers: &HeaderMap,
    action: GatewayAction,
    stream_id: Option<&RecordingIngestStreamId>,
    internal_path: String,
    mut envelope: impl AuthorizedEnvelope,
) -> Response {
    let started_at = Instant::now();
    let catalog = current_catalog(&state.catalog);
    let Some(resource) = catalog.single_recording_ingest_resource() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (subject, producer) = match authenticate(state, resource, headers, started_at).await {
        Ok(authenticated) => authenticated,
        Err(response) => return response,
    };
    let trace_id = match TraceId::new(uuid::Uuid::new_v4().to_string()) {
        Ok(trace_id) => trace_id,
        Err(error) => return auth_audit_error_response(error.into()),
    };
    let decision = catalog.decide_recording_ingest(RecordingIngestPolicyRequest {
        principal: &subject.principal,
        resource,
        producer: &producer,
        action,
        trace_id: &trace_id,
    });
    let mut audit_metadata = BTreeMap::from([
        ("action".to_owned(), format!("{action:?}")),
        ("producer_id".to_owned(), producer.id.to_string()),
        ("trace_id".to_owned(), trace_id.to_string()),
    ]);
    if let Some(policy) = &decision.policy_version {
        audit_metadata.insert("policy_version".to_owned(), policy.to_string());
    }
    if let Some(rule) = &decision.rule_id {
        audit_metadata.insert("policy_rule".to_owned(), rule.to_string());
    }
    if let Some(stream_id) = stream_id {
        audit_metadata.insert("stream_id".to_owned(), stream_id.to_string());
    }
    let allowed = decision.effect == PolicyEffect::Allow;
    if let Err(error) = record_resource_auth_audit(
        &state.gateway_state,
        AuthAuditTarget {
            profile: None,
            protected_resource: &resource.protected_resource,
        },
        if allowed {
            AuthOutcome::Allow
        } else {
            AuthOutcome::Deny
        },
        if allowed {
            AuthReasonCode::AuthAllow
        } else {
            AuthReasonCode::PolicyDenied
        },
        Some(&subject),
        started_at,
        audit_metadata,
    )
    .await
    {
        return auth_audit_error_response(error);
    }
    if !allowed {
        return ingest_error(
            StatusCode::FORBIDDEN,
            IngestErrorCode::Forbidden,
            "recording ingest policy denied the request",
        );
    }

    envelope.set_producer(authorized_producer(&producer));
    let expires_at = std::cmp::min(
        subject.access_token.expires_at,
        Utc::now() + TimeDelta::seconds(INTERNAL_TOKEN_TTL_SECONDS),
    );
    let internal_token = match state.internal_token_issuer.issue_resource(
        resource.protected_resource.clone(),
        match ServerSlug::new("recording-hub") {
            Ok(server) => server,
            Err(error) => return auth_audit_error_response(error.into()),
        },
        subject.principal,
        expires_at,
    ) {
        Ok(token) => token,
        Err(error) => return auth_audit_error_response(error.into()),
    };
    let url = format!(
        "{}{}",
        resource.upstream.url.as_str().trim_end_matches('/'),
        internal_path
    );
    let response = match current_http_client(&state.http)
        .request(
            if action == GatewayAction::RecordingBatchAppend {
                reqwest::Method::PUT
            } else {
                reqwest::Method::POST
            },
            url,
        )
        .bearer_auth(internal_token.bearer_token)
        .header(header::CONTENT_TYPE.as_str(), MEDIA_TYPE)
        .body(envelope.encode_to_vec())
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!("recording hub ingest request failed: {error}");
            return ingest_error(
                StatusCode::SERVICE_UNAVAILABLE,
                IngestErrorCode::StorageUnavailable,
                "recording hub is unavailable",
            );
        }
    };
    let status = response.status();
    let body = match response.bytes().await {
        Ok(body) => body,
        Err(error) => {
            tracing::warn!("recording hub ingest response failed: {error}");
            return ingest_error(
                StatusCode::BAD_GATEWAY,
                IngestErrorCode::StorageUnavailable,
                "recording hub returned an invalid response",
            );
        }
    };
    let status = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut forwarded = (status, body).into_response();
    forwarded
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(MEDIA_TYPE));
    forwarded
}

async fn authenticate(
    state: &RecordingIngestGatewayState,
    resource: &RecordingIngestResource,
    headers: &HeaderMap,
    started_at: Instant,
) -> Result<(AuthenticatedSubject, RecordingProducerRegistration), Response> {
    let audit_target = AuthAuditTarget {
        profile: None,
        protected_resource: &resource.protected_resource,
    };
    let Some(authorization_server) = current_catalog(&state.catalog)
        .authorization_server(&resource.authorization_server)
        .cloned()
    else {
        return Err(record_denial(
            state,
            audit_target,
            AuthReasonCode::UnknownAuthorizationServer,
            started_at,
            StatusCode::SERVICE_UNAVAILABLE,
            "authorization server is unavailable",
        )
        .await);
    };
    let Some(raw_authorization) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return Err(record_denial(
            state,
            audit_target,
            AuthReasonCode::MissingAuthorizationHeader,
            started_at,
            StatusCode::UNAUTHORIZED,
            "bearer token is required",
        )
        .await);
    };
    let token = match BearerToken::from_authorization_header(raw_authorization) {
        Ok(token) => token,
        Err(_) => {
            return Err(record_denial(
                state,
                audit_target,
                AuthReasonCode::InvalidAuthorizationHeader,
                started_at,
                StatusCode::UNAUTHORIZED,
                "bearer token is invalid",
            )
            .await);
        }
    };
    let jwks = match load_jwks(
        &current_http_client(&state.http),
        &authorization_server.jwks,
    )
    .await
    {
        Ok(jwks) => jwks,
        Err(_) => {
            return Err(record_denial(
                state,
                audit_target,
                AuthReasonCode::AuthorizationServerUnavailable,
                started_at,
                StatusCode::SERVICE_UNAVAILABLE,
                "authorization server is unavailable",
            )
            .await);
        }
    };
    let auth_config = match JwtAuthConfig::new(
        authorization_server.issuer,
        resource.protected_resource.clone(),
        resource.required_scopes.clone(),
        allowed_gateway_jwt_algorithms(),
    ) {
        Ok(config) => config,
        Err(error) => return Err(auth_audit_error_response(error.into())),
    };
    let subject = match JwtVerifier::new(auth_config, jwks).verify(&token) {
        Ok(subject) => subject,
        Err(_) => {
            return Err(record_denial(
                state,
                audit_target,
                AuthReasonCode::InvalidBearerToken,
                started_at,
                StatusCode::UNAUTHORIZED,
                "bearer token is invalid",
            )
            .await);
        }
    };
    if subject.principal.kind != PrincipalKind::Service {
        return Err(record_denial(
            state,
            audit_target,
            AuthReasonCode::InvalidBearerToken,
            started_at,
            StatusCode::FORBIDDEN,
            "recording ingest requires a service principal",
        )
        .await);
    }
    let catalog = current_catalog(&state.catalog);
    let Some(producer) =
        catalog.recording_producer_for_client(resource, &subject.access_token.oauth_client_id)
    else {
        return Err(record_denial(
            state,
            audit_target,
            AuthReasonCode::InvalidClient,
            started_at,
            StatusCode::FORBIDDEN,
            "OAuth client is not a registered recording producer",
        )
        .await);
    };
    Ok((subject, producer.clone()))
}

async fn record_denial(
    state: &RecordingIngestGatewayState,
    target: AuthAuditTarget<'_>,
    reason: AuthReasonCode,
    started_at: Instant,
    status: StatusCode,
    message: &str,
) -> Response {
    if let Err(error) = record_resource_auth_audit(
        &state.gateway_state,
        target,
        AuthOutcome::Deny,
        reason,
        None,
        started_at,
        BTreeMap::new(),
    )
    .await
    {
        return auth_audit_error_response(error);
    }
    ingest_error(status, IngestErrorCode::Unauthorized, message)
}

fn authorized_producer(producer: &RecordingProducerRegistration) -> AuthorizedRecordingProducer {
    AuthorizedRecordingProducer {
        producer_id: producer.id.to_string(),
        oauth_client_id: producer.oauth_client.to_string(),
        tenant_id: producer.tenant.to_string(),
        dataset: producer.dataset.to_string(),
        allowed_application_ids: producer
            .allowed_application_ids
            .iter()
            .map(ToString::to_string)
            .collect(),
        classification: producer.classification.clone(),
        labels: producer.labels.iter().map(ToString::to_string).collect(),
        maximum_stream_bytes: producer.quotas.maximum_stream_bytes,
    }
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
        ));
    }
    T::decode(body).map_err(|_| {
        ingest_error(
            StatusCode::BAD_REQUEST,
            IngestErrorCode::InvalidRequest,
            "protobuf request is invalid",
        )
    })
}

fn protobuf_response(status: StatusCode, message: &impl Message) -> Response {
    let mut response = (status, message.encode_to_vec()).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(MEDIA_TYPE));
    response
}

fn ingest_error(status: StatusCode, code: IngestErrorCode, message: &str) -> Response {
    protobuf_response(
        status,
        &IngestError {
            code: code.into(),
            message: message.to_owned(),
            expected_sequence: None,
            retry_after_seconds: None,
        },
    )
}

fn stream_not_found() -> Response {
    ingest_error(
        StatusCode::NOT_FOUND,
        IngestErrorCode::StreamNotFound,
        "recording ingest stream was not found",
    )
}
