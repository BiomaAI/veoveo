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

#[derive(Clone)]
pub(super) struct ForwardedBearer(pub(super) String);

pub(super) async fn authenticate_internal_mcp(
    State(state): State<InternalMcpAuthState>,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
    let identity = match verify_authorization(&state.verifier, request.headers()) {
        Ok(identity) => identity,
        Err(error) => {
            tracing::warn!(%error, "rejected SUMO MCP request");
            return (StatusCode::UNAUTHORIZED, "invalid gateway authorization").into_response();
        }
    };
    let forwarded = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|header| header.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, token)| token.to_owned());
    if let Some(token) = forwarded {
        request.extensions_mut().insert(ForwardedBearer(token));
    }
    request.extensions_mut().insert(identity);
    next.run(request).await
}

fn verify_authorization(
    verifier: &GatewayInternalTokenVerifier,
    headers: &HeaderMap,
) -> Result<GatewayInternalIdentity, String> {
    let header = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| "missing internal authorization".to_owned())?;
    let (scheme, token) = header
        .split_once(' ')
        .ok_or_else(|| "missing bearer token".to_owned())?;
    if !scheme.eq_ignore_ascii_case("bearer")
        || token.is_empty()
        || token.chars().any(char::is_whitespace)
    {
        return Err("invalid bearer token".to_owned());
    }
    verifier.verify(token).map_err(|error| error.to_string())
}
