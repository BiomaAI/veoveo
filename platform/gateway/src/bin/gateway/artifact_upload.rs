//! Public upload policy enforcement and bounded streaming proxy.

use axum::{
    Json, Router,
    body::Body,
    extract::{Extension, MatchedPath, Path, Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use chrono::{TimeDelta, Utc};
use serde::Deserialize;
use std::{collections::BTreeMap, num::NonZeroU32, time::Instant};
use veoveo_mcp_contract::{self as contract, UploadErrorCode as Code};
use veoveo_mcp_gateway::{AuthenticatedSubject, PolicyRequest, merge_principal_audit_metadata};

use crate::runtime::{ArtifactHttpState, current_catalog};

pub(super) fn router(state: ArtifactHttpState) -> Router {
    Router::new()
        .route("/artifacts/{profile}/upload-policy", get(proxy))
        .route("/artifacts/{profile}/uploads", post(proxy))
        .route(
            "/artifacts/{profile}/uploads/{upload_id}",
            get(proxy).delete(proxy),
        )
        .route(
            "/artifacts/{profile}/uploads/{upload_id}/parts/{part_number}",
            put(proxy),
        )
        .route(
            "/artifacts/{profile}/uploads/{upload_id}/complete",
            post(proxy),
        )
        .with_state(state)
}

#[derive(Deserialize)]
struct Route {
    profile: contract::GatewayProfileId,
    upload_id: Option<contract::ArtifactUploadId>,
    part_number: Option<NonZeroU32>,
}

async fn proxy(
    State(state): State<ArtifactHttpState>,
    Extension(subject): Extension<AuthenticatedSubject>,
    route: Result<Path<Route>, axum::extract::rejection::PathRejection>,
    matched: MatchedPath,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let uri = parts.uri;
    let method = parts.method;
    let headers = parts.headers;
    let Path(route) = match route {
        Ok(route) => route,
        Err(_) => return fault(Code::Malformed),
    };
    let policy_request = matched.as_str().ends_with("/upload-policy");
    let started = Instant::now();
    let catalog = current_catalog(&state.catalog);
    let Some(profile) = catalog.profile(&route.profile) else {
        return fault(Code::NotFound);
    };
    let Ok(trace_id) = contract::TraceId::new(uuid::Uuid::now_v7().to_string()) else {
        return fault(Code::Unavailable);
    };
    let target = contract::PolicyTarget::Server {
        server: state.artifact_server.clone(),
    };
    let mut decision = catalog.decide(PolicyRequest {
        principal: &subject.principal,
        profile: &route.profile,
        action: contract::GatewayAction::ArtifactUpload,
        target: &target,
        trace_id: &trace_id,
    });
    let explanation = if profile.artifact_upload.is_none() {
        Some("Uploads have not been enabled for this Work Context.")
    } else if !subject
        .actor
        .scopes
        .iter()
        .any(|scope| scope.as_str() == "artifact:upload")
    {
        decision.reason = contract::PolicyReasonCode::MissingScope;
        Some("Your sign-in does not include upload access. Sign in again after access is granted.")
    } else if !subject
        .authority
        .membership
        .allows(contract::WorkContextMembershipLevel::Contributor)
    {
        decision.reason = contract::PolicyReasonCode::MissingRole;
        Some("You need contributor access to upload files here.")
    } else if decision.effect != contract::PolicyEffect::Allow {
        Some("Current access does not allow uploads in this Work Context.")
    } else {
        None
    };
    if explanation.is_some() {
        decision.effect = contract::PolicyEffect::Deny;
    }
    let Ok(audit_method) = contract::McpMethodName::new("artifact/upload") else {
        return fault(Code::Unavailable);
    };
    if let Err(error) = state
        .gateway_state
        .record_audit_event(&contract::AuditEvent {
            event_id: trace_id.clone(),
            timestamp: decision.evaluated_at,
            trace_id,
            profile: route.profile.clone(),
            method: audit_method,
            action: contract::GatewayAction::ArtifactUpload,
            target,
            decision: decision.clone(),
            principal: Some(subject.principal.id.clone()),
            principal_attributes: Some(contract::PrincipalAuditAttributes::from(
                &subject.principal,
            )),
            tenant: subject.principal.tenant.clone(),
            token_issuer: Some(subject.access_token.issuer.clone()),
            latency_ms: u64::try_from(started.elapsed().as_millis()).ok(),
            metadata: merge_principal_audit_metadata(
                BTreeMap::from([
                    ("http_method".into(), method.to_string()),
                    ("route".into(), matched.as_str().to_owned()),
                ]),
                &subject.principal,
            ),
        })
        .await
    {
        tracing::error!(%error, "could not persist upload policy decision");
        return fault(Code::Unavailable);
    }
    if let Some(explanation) = explanation {
        if !policy_request {
            return fault(Code::Denied);
        }
        let destination_name = catalog
            .work_context(&subject.authority.work_context)
            .map(|context| context.title.clone())
            .unwrap_or_else(|| "Current Work Context".into());
        return (
            [(header::CACHE_CONTROL, "no-store")],
            Json(contract::EffectiveArtifactUploadPolicy {
                allowed: false,
                explanation: explanation.into(),
                actor: subject.actor.id,
                work_context: subject.authority.work_context,
                destination_name,
                access_description:
                    "Ownership and sharing follow this Work Context's configured access.".into(),
                policy: None,
                available_bytes: None,
            }),
        )
            .into_response();
    }
    let configuration_sha256 = hex::encode(catalog.configuration_sha256());
    drop(catalog);
    let version = match state
        .gateway_state
        .platform_store()
        .artifact_upload_authority_version(
            subject.authority.tenant.as_str(),
            subject.authority.work_context.as_str(),
            route.profile.as_str(),
        )
        .await
    {
        Ok(Some(version)) => version,
        Ok(None) => return fault(Code::Denied),
        Err(error) => {
            tracing::error!(%error, "upload authority unavailable");
            return fault(Code::Unavailable);
        }
    };
    if version.control_plane_sha256 != configuration_sha256 {
        return fault(Code::Unavailable);
    }
    let binding = match (
        contract::UploadSha256::parse(configuration_sha256),
        contract::UploadSha256::parse(version.context_digest),
    ) {
        (Ok(control_plane_sha256), Ok(context_digest)) => contract::ArtifactUploadAuthority {
            control_plane_sha256,
            context_digest,
        },
        _ => return fault(Code::Unavailable),
    };
    let expires = std::cmp::min(
        subject.access_token.expires_at,
        Utc::now() + TimeDelta::seconds(60),
    );
    let token = match state.internal_token_issuer.issue_artifact_upload(
        route.profile,
        subject.actor,
        subject.authority,
        binding,
        expires,
    ) {
        Ok(token) => token,
        Err(_) => return fault(Code::Unauthenticated),
    };
    let path = if policy_request {
        "/artifact-uploads/policy".into()
    } else if let Some(id) = route.upload_id {
        if let Some(number) = route.part_number {
            format!("/artifact-uploads/{id}/parts/{number}")
        } else if matched.as_str().ends_with("/complete") {
            format!("/artifact-uploads/{id}/complete")
        } else {
            format!("/artifact-uploads/{id}")
        }
    } else {
        "/artifact-uploads".into()
    };
    let mut url = format!("{}{path}", state.artifact_service_url);
    if method == Method::GET
        && let Some(query) = uri.query()
    {
        url.push('?');
        url.push_str(query);
    }
    let mut request = state
        .http
        .request(method, url)
        .bearer_auth(token)
        .body(reqwest::Body::wrap_stream(body.into_data_stream()));
    for name in [
        "idempotency-key",
        contract::UPLOAD_PART_BYTE_LEN_HEADER,
        contract::UPLOAD_PART_SHA256_HEADER,
        "content-type",
        "content-length",
    ] {
        for value in headers.get_all(name) {
            request = request.header(name, value);
        }
    }
    match request.send().await {
        Ok(upstream) if !upstream.status().is_redirection() => response(upstream),
        Ok(_) => fault(Code::Unavailable),
        Err(error) => {
            tracing::warn!(error = %error.without_url(), "upload upstream request interrupted");
            fault(Code::Unavailable)
        }
    }
}

fn response(upstream: reqwest::Response) -> Response {
    let status = upstream.status();
    let mut headers = HeaderMap::new();
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::RETRY_AFTER,
    ] {
        if let Some(value) = upstream.headers().get(&name) {
            headers.insert(name, value.clone());
        }
    }
    headers.insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    (status, headers, Body::from_stream(upstream.bytes_stream())).into_response()
}

fn fault(code: Code) -> Response {
    let message = match code {
        Code::Malformed => "The upload request is invalid.",
        Code::NotFound => "This upload is unavailable.",
        Code::Denied => "Current access does not allow this upload.",
        Code::Unauthenticated => "Sign in to continue this upload.",
        _ => "The upload service is temporarily unavailable.",
    };
    (
        StatusCode::from_u16(code.http_status()).unwrap_or(StatusCode::SERVICE_UNAVAILABLE),
        [(header::CACHE_CONTROL, "no-store")],
        Json(contract::ArtifactUploadError {
            code,
            message: message.into(),
            request_id: contract::ArtifactUploadRequestId::new(),
            required_bytes: None,
            available_bytes: None,
        }),
    )
        .into_response()
}
