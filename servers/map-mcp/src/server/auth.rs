use axum::{
    extract::{Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::IntoResponse,
};
use veoveo_mcp_contract::{GatewayInternalIdentity, GatewayInternalTokenVerifier};

#[derive(Clone)]
pub(crate) struct ForwardedBearer(pub String);

#[derive(Clone)]
pub(super) struct InternalAuthState {
    pub verifier: GatewayInternalTokenVerifier,
}

#[derive(Clone)]
pub(super) struct AdminAuthState {
    pub required_scope: String,
}

pub(super) async fn authenticate_internal(
    State(state): State<InternalAuthState>,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
    let header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    let token = header.and_then(bearer_token).map(ToOwned::to_owned);
    let identity = token
        .as_deref()
        .and_then(|token| state.verifier.verify(token).ok());
    let (Some(token), Some(identity)) = (token, identity) else {
        tracing::warn!("rejected unsigned or invalid Map request");
        return (StatusCode::UNAUTHORIZED, "invalid gateway authorization").into_response();
    };
    request.extensions_mut().insert(ForwardedBearer(token));
    request.extensions_mut().insert(identity);
    next.run(request).await
}

pub(super) async fn authorize_admin(
    State(state): State<AdminAuthState>,
    request: Request,
    next: Next,
) -> axum::response::Response {
    let allowed = request
        .extensions()
        .get::<GatewayInternalIdentity>()
        .is_some_and(|identity| {
            identity
                .principal
                .scopes
                .iter()
                .any(|scope| scope.as_str() == state.required_scope)
        });
    if !allowed {
        return (StatusCode::FORBIDDEN, "map administrative scope required").into_response();
    }
    next.run(request).await
}

fn bearer_token(header: &str) -> Option<&str> {
    let (scheme, token) = header.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer")
        && !token.is_empty()
        && !token.chars().any(char::is_whitespace))
    .then_some(token)
}
