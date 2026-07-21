use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::IntoResponse,
};
use veoveo_mcp_contract::{GatewayInternalIdentity, GatewayInternalTokenVerifier};

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
            tracing::warn!("rejected reason MCP request: {message}");
            return (StatusCode::UNAUTHORIZED, "invalid gateway authorization").into_response();
        }
    };
    if let Some(token) = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|header| internal_bearer_token(header).ok())
        .map(str::to_owned)
    {
        request.extensions_mut().insert(ForwardedBearer(token));
    }
    request.extensions_mut().insert(identity);
    next.run(request).await
}

fn verify_internal_authorization(
    verifier: &GatewayInternalTokenVerifier,
    headers: &HeaderMap,
) -> Result<GatewayInternalIdentity, String> {
    let header = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| "missing internal authorization".to_owned())?;
    let token = internal_bearer_token(header).map_err(str::to_owned)?;
    verifier.verify(token).map_err(|error| error.to_string())
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
