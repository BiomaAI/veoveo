use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::IntoResponse,
};
use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_mcp_contract::{GatewayInternalIdentity, GatewayInternalTokenVerifier, PlaneCaller};

#[derive(Clone)]
pub(super) struct ForwardedBearer(pub(super) String);

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
    request.extensions_mut().insert(ForwardedBearer(token));
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

pub(super) fn caller(context: &RequestContext<RoleServer>) -> Result<PlaneCaller, McpError> {
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| McpError::invalid_request("authenticated HTTP context missing", None))?;
    let identity = parts
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(|| McpError::invalid_request("gateway identity missing", None))?;
    let bearer = parts
        .extensions
        .get::<ForwardedBearer>()
        .map(|bearer| bearer.0.clone())
        .ok_or_else(|| McpError::invalid_request("forwarded bearer missing", None))?;
    Ok(PlaneCaller {
        memberships: identity.actor.group_memberships(),
        identity,
        bearer_token: bearer,
    })
}

fn bearer(headers: &HeaderMap) -> Result<&str, &'static str> {
    let header = headers
        .get(AUTHORIZATION)
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
