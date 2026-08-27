use std::{collections::BTreeMap, time::Instant};

use axum::{
    body::Body,
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse as _, Response},
};
use chrono::{TimeDelta, Utc};
use veoveo_mcp_contract::{
    AuditEvent, GatewayAction, GatewayProfileId, McpMethodName, PolicyEffect, PolicyTarget,
    PrincipalAuditAttributes, ServerSlug, TraceId,
};
use veoveo_mcp_gateway::{AuthenticatedSubject, PolicyRequest, merge_principal_audit_metadata};

use crate::runtime::{RecordingLayerPublicationState, current_catalog, current_http_client};

const RECORDING_SERVER: &str = "recording";
const INTERNAL_PUBLICATION_TOKEN_TTL_SECONDS: i64 = 60;

pub(super) async fn publish_recording_layer(
    State(state): State<RecordingLayerPublicationState>,
    Path(profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let started_at = Instant::now();
    let Ok(profile) = GatewayProfileId::new(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(recording_server) = ServerSlug::new(RECORDING_SERVER) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let catalog = current_catalog(&state.catalog);
    if catalog.profile(&profile).is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let trace_id = match TraceId::new(uuid::Uuid::new_v4().to_string()) {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "failed to create recording publication trace id");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let target = PolicyTarget::Server {
        server: recording_server,
    };
    let decision = catalog.decide(PolicyRequest {
        principal: &subject.principal,
        profile: &profile,
        action: GatewayAction::RecordingLayerPublish,
        target: &target,
        trace_id: &trace_id,
    });
    let audit = AuditEvent {
        event_id: trace_id.clone(),
        timestamp: decision.evaluated_at,
        trace_id,
        profile: profile.clone(),
        method: McpMethodName::new("recording/layer_publish")
            .expect("static publication audit method is valid"),
        action: GatewayAction::RecordingLayerPublish,
        target,
        decision: decision.clone(),
        principal: Some(subject.principal.id.clone()),
        principal_attributes: Some(PrincipalAuditAttributes::from(&subject.principal)),
        tenant: subject.principal.tenant.clone(),
        token_issuer: Some(subject.access_token.issuer.clone()),
        latency_ms: u64::try_from(started_at.elapsed().as_millis()).ok(),
        metadata: merge_principal_audit_metadata(BTreeMap::new(), &subject.principal),
    };
    if let Err(error) = state.gateway_state.record_audit_event(&audit).await {
        tracing::error!(%error, "failed to audit recording layer publication");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if decision.effect != PolicyEffect::Allow {
        return StatusCode::FORBIDDEN.into_response();
    }
    let descriptor = match headers.get("x-artifact-stream-put") {
        Some(value) => value.clone(),
        None => return StatusCode::BAD_REQUEST.into_response(),
    };
    let content_length = match headers.get(header::CONTENT_LENGTH) {
        Some(value) => value.clone(),
        None => return StatusCode::LENGTH_REQUIRED.into_response(),
    };
    let expires_at = std::cmp::min(
        subject.access_token.expires_at,
        Utc::now() + TimeDelta::seconds(INTERNAL_PUBLICATION_TOKEN_TTL_SECONDS),
    );
    let token = match state.internal_token_issuer.issue(
        profile,
        state.artifact_server,
        subject.actor,
        subject.authority,
        expires_at,
    ) {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "failed to issue Artifact publication token");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };
    drop(catalog);
    let upstream = current_http_client(&state.http)
        .post(format!(
            "{}/artifacts/stream",
            state.artifact_service_url.trim_end_matches('/')
        ))
        .bearer_auth(token.bearer_token)
        .header("x-artifact-stream-put", descriptor)
        .header(header::CONTENT_LENGTH, content_length)
        .body(reqwest::Body::wrap_stream(body.into_data_stream()))
        .send()
        .await;
    let upstream = match upstream {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "recording layer Artifact publication failed");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    let status = upstream.status();
    let content_type = upstream.headers().get(header::CONTENT_TYPE).cloned();
    let response_body = Body::from_stream(upstream.bytes_stream());
    let mut response = Response::new(response_body);
    *response.status_mut() = status;
    if let Some(value) = content_type {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    response
}
