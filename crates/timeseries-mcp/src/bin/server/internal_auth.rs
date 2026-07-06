use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::IntoResponse,
};
use veoveo_mcp_contract::{GatewayInternalIdentity, GatewayInternalTokenVerifier};

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
            tracing::warn!("rejected timeseries MCP request: {message}");
            return (StatusCode::UNAUTHORIZED, "invalid gateway authorization").into_response();
        }
    };
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
