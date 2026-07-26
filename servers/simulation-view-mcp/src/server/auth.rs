use axum::{
    extract::{Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::IntoResponse,
};
use veoveo_mcp_contract::GatewayInternalTokenVerifier;

#[derive(Clone)]
pub(super) struct InternalAuthState {
    pub verifier: GatewayInternalTokenVerifier,
}

pub(super) async fn authenticate_internal(
    State(state): State<InternalAuthState>,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
    let identity = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(bearer_token)
        .and_then(|token| state.verifier.verify(token).ok());
    let Some(identity) = identity else {
        tracing::warn!("rejected unsigned or invalid Simulation View request");
        return (StatusCode::UNAUTHORIZED, "invalid gateway authorization").into_response();
    };
    request.extensions_mut().insert(identity);
    next.run(request).await
}

fn bearer_token(header: &str) -> Option<&str> {
    let (scheme, token) = header.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer")
        && !token.is_empty()
        && !token.chars().any(char::is_whitespace))
    .then_some(token)
}
