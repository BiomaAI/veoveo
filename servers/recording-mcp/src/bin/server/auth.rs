use axum::{
    extract::{Request, State},
    http::{
        HeaderMap, StatusCode,
        header::{AUTHORIZATION, HOST},
    },
    middleware::Next,
    response::IntoResponse,
};
use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use std::sync::Arc;
use veoveo_mcp_contract::{
    GatewayInternalIdentity, GatewayInternalTokenVerifier, PlaneCaller, host_authority_is_allowed,
    parse_request_host_authority,
};

pub(super) const ARTIFACT_READ_AUTHORIZATION_HEADER: &str = "x-veoveo-artifact-read-authorization";

#[derive(Clone)]
pub(super) struct InternalAuthState {
    pub(super) verifier: GatewayInternalTokenVerifier,
    pub(super) allowed_hosts: Arc<Vec<String>>,
}

pub(super) async fn authenticate(
    State(state): State<InternalAuthState>,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
    let host = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_request_host_authority)
        .or_else(|| {
            request
                .uri()
                .authority()
                .and_then(|value| parse_request_host_authority(value.as_str()))
        });
    if !host.is_some_and(|host| host_authority_is_allowed(&host, &state.allowed_hosts)) {
        return (StatusCode::MISDIRECTED_REQUEST, "untrusted Host authority").into_response();
    }
    let token = match bearer(request.headers()) {
        Ok(token) => token.to_owned(),
        Err(message) => {
            tracing::warn!("rejected recording MCP request: {message}");
            return (StatusCode::UNAUTHORIZED, "invalid gateway authorization").into_response();
        }
    };
    let identity = match state.verifier.verify(&token) {
        Ok(identity) => identity,
        Err(error) => {
            tracing::warn!("rejected recording MCP request: {error}");
            return (StatusCode::UNAUTHORIZED, "invalid gateway authorization").into_response();
        }
    };
    request.extensions_mut().insert(identity);
    next.run(request).await
}

pub(super) fn identity(
    context: &RequestContext<RoleServer>,
) -> Result<GatewayInternalIdentity, McpError> {
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    parts
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })
}

pub(super) fn artifact_caller(
    identity: GatewayInternalIdentity,
    headers: &HeaderMap,
) -> Result<PlaneCaller, &'static str> {
    let bearer = bearer_from_name(headers, ARTIFACT_READ_AUTHORIZATION_HEADER)?;
    Ok(PlaneCaller {
        memberships: identity.actor.group_memberships(),
        identity,
        bearer_token: bearer.to_owned(),
    })
}

pub(super) fn artifact_caller_from_context(
    context: &RequestContext<RoleServer>,
    identity: GatewayInternalIdentity,
) -> Result<PlaneCaller, McpError> {
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    artifact_caller(identity, &parts.headers)
        .map_err(|_| McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None))
}

fn bearer(headers: &HeaderMap) -> Result<&str, &'static str> {
    bearer_from_name(headers, AUTHORIZATION)
}

fn bearer_from_name(
    headers: &HeaderMap,
    name: impl axum::http::header::AsHeaderName,
) -> Result<&str, &'static str> {
    let header = headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .ok_or("missing authorization")?;
    let Some((scheme, token)) = header.split_once(' ') else {
        return Err("missing bearer token");
    };
    if !scheme.eq_ignore_ascii_case("bearer")
        || token.is_empty()
        || token.chars().any(char::is_whitespace)
    {
        return Err("invalid bearer token");
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn untrusted_hosts_are_rejected_before_authentication() {
        use axum::{Router, body::Body, middleware, routing::post};
        use tower::ServiceExt;
        use veoveo_mcp_contract::{GatewayInternalTrustBundle, ServerSlug, TokenIssuer};
        let state = InternalAuthState {
            verifier: GatewayInternalTokenVerifier::new(
                TokenIssuer::new("veoveo-internal").unwrap(),
                ServerSlug::new("recording").unwrap(),
                GatewayInternalTrustBundle::from_json(r#"{"keys":[{"kty":"OKP","crv":"Ed25519","x":"OMOoJJu_AQS7UM8u2GVtMVj8W1zcE6QhR0DMBr9HEcg","alg":"EdDSA","use":"sig","kid":"test-key"}]}"#).unwrap(),
            ),
            allowed_hosts: Arc::new(vec!["recording-mcp:8796".into()]),
        };
        let app = Router::new()
            .route("/mcp", post(|| async { StatusCode::NO_CONTENT }))
            .layer(middleware::from_fn_with_state(state, authenticate));
        for (host, expected) in [
            (None, StatusCode::MISDIRECTED_REQUEST),
            (Some("untrusted.invalid"), StatusCode::MISDIRECTED_REQUEST),
            (Some("recording-mcp:9999"), StatusCode::MISDIRECTED_REQUEST),
            (
                Some("recording-mcp:not-a-port"),
                StatusCode::MISDIRECTED_REQUEST,
            ),
            (Some("recording-mcp:8796"), StatusCode::UNAUTHORIZED),
            (Some("RECORDING-MCP:8796"), StatusCode::UNAUTHORIZED),
        ] {
            for bearer in [None, Some("Bearer invalid-token")] {
                let mut request = Request::builder().method("POST").uri("/mcp");
                if let Some(host) = host {
                    request = request.header(HOST, host);
                }
                if let Some(bearer) = bearer {
                    request = request.header(AUTHORIZATION, bearer);
                }
                let response = app
                    .clone()
                    .oneshot(request.body(Body::empty()).unwrap())
                    .await
                    .unwrap();
                assert_eq!(response.status(), expected, "host {host:?}");
            }
        }
    }

    #[test]
    fn bearer_parser_is_strict() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "Bearer one.two.three".parse().unwrap());
        assert_eq!(bearer(&headers), Ok("one.two.three"));
        headers.insert(AUTHORIZATION, "Basic one.two.three".parse().unwrap());
        assert!(bearer(&headers).is_err());
    }
}
