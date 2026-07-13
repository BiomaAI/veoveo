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
            tracing::warn!("rejected artifact MCP request: {message}");
            return (StatusCode::UNAUTHORIZED, "invalid gateway authorization").into_response();
        }
    };
    let identity = match state.verifier.verify(&token) {
        Ok(identity) => identity,
        Err(error) => {
            tracing::warn!("rejected artifact MCP request: {error}");
            return (StatusCode::UNAUTHORIZED, "invalid gateway authorization").into_response();
        }
    };
    request.extensions_mut().insert(ForwardedBearer(token));
    request.extensions_mut().insert(identity);
    next.run(request).await
}

fn bearer(headers: &HeaderMap) -> Result<&str, &'static str> {
    let value = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or("missing authorization")?;
    let Some((scheme, token)) = value.split_once(' ') else {
        return Err("missing bearer token");
    };
    if !scheme.eq_ignore_ascii_case("bearer") {
        return Err("authorization scheme must be Bearer");
    }
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        return Err("bearer token contains invalid whitespace");
    }
    Ok(token)
}

pub(super) fn caller(context: &RequestContext<RoleServer>) -> Result<PlaneCaller, McpError> {
    let parts = request_parts(context)?;
    let identity = parts
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(|| McpError::invalid_request("gateway identity missing", None))?;
    let bearer_token = parts
        .extensions
        .get::<ForwardedBearer>()
        .map(|value| value.0.clone())
        .ok_or_else(|| McpError::invalid_request("forwarded bearer missing", None))?;
    Ok(PlaneCaller {
        memberships: identity.principal.group_memberships(),
        identity,
        bearer_token,
    })
}

fn request_parts(
    context: &RequestContext<RoleServer>,
) -> Result<&axum::http::request::Parts, McpError> {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| McpError::invalid_request("authenticated HTTP context missing", None))
}
