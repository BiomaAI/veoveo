use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::IntoResponse,
};
use veoveo_mcp_contract::{GatewayInternalIdentity, GatewayInternalTokenVerifier};

/// The raw gateway bearer this server received, captured so it can be forwarded
/// to the shared artifact plane on the caller's behalf (the plane accepts tokens
/// audienced to `duckdb`). Never logged.
#[derive(Clone)]
pub(super) struct ForwardedBearer(pub(super) String);

#[derive(Clone)]
pub(super) struct InternalMcpAuthState {
    pub(super) verifier: GatewayInternalTokenVerifier,
}

pub(super) async fn authenticate_internal_mcp(
    State(state): State<InternalMcpAuthState>,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
    let identity = match verify_internal_authorization(&state.verifier, request.headers()) {
        Ok(identity) => identity,
        Err(message) => {
            tracing::warn!("rejected duckdb MCP request: {message}");
            return (StatusCode::UNAUTHORIZED, "invalid gateway authorization").into_response();
        }
    };
    if let Some(token) = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|h| internal_bearer_token(h).ok())
        .map(str::to_string)
    {
        request.extensions_mut().insert(ForwardedBearer(token));
    }
    request
        .extensions_mut()
        .insert::<GatewayInternalIdentity>(identity);
    next.run(request).await
}

pub(super) fn verify_internal_authorization(
    verifier: &GatewayInternalTokenVerifier,
    headers: &HeaderMap,
) -> Result<GatewayInternalIdentity, String> {
    let header = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| "missing internal authorization".to_string())?;
    let token = internal_bearer_token(header).map_err(str::to_string)?;
    verifier.verify(token).map_err(|err| err.to_string())
}

/// Extract the raw bearer token from request headers, for forwarding to the
/// artifact plane. Returns `None` when absent or malformed.
pub(super) fn bearer_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|h| internal_bearer_token(h).ok())
        .map(str::to_string)
}

fn internal_bearer_token(header: &str) -> Result<&str, &'static str> {
    let Some((scheme, token)) = header.split_once(' ') else {
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
