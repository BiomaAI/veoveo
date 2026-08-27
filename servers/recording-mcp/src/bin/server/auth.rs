use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::IntoResponse,
};
use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_mcp_contract::{GatewayInternalIdentity, GatewayInternalTokenVerifier, PlaneCaller};

pub(super) const ARTIFACT_READ_AUTHORIZATION_HEADER: &str = "x-veoveo-artifact-read-authorization";

#[derive(Clone)]
pub(super) struct InternalAuthState {
    pub(super) verifier: GatewayInternalTokenVerifier,
}

pub(super) async fn authenticate(
    State(state): State<InternalAuthState>,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
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
        .ok_or_else(|| McpError::invalid_request("authenticated HTTP context missing", None))?;
    parts
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(|| McpError::invalid_request("gateway identity missing", None))
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
        .ok_or_else(|| McpError::invalid_request("authenticated HTTP context missing", None))?;
    artifact_caller(identity, &parts.headers)
        .map_err(|_| McpError::invalid_request("Artifact read authority missing", None))
}

fn bearer(headers: &HeaderMap) -> Result<&str, &'static str> {
    bearer_from_name(headers, AUTHORIZATION)
}

fn bearer_from_name<'a>(
    headers: &'a HeaderMap,
    name: impl axum::http::header::AsHeaderName,
) -> Result<&'a str, &'static str> {
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

    #[test]
    fn bearer_parser_is_strict() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "Bearer one.two.three".parse().unwrap());
        assert_eq!(bearer(&headers), Ok("one.two.three"));
        headers.insert(AUTHORIZATION, "Basic one.two.three".parse().unwrap());
        assert!(bearer(&headers).is_err());
    }
}
