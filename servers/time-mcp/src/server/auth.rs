use axum::{
    extract::{Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::IntoResponse,
};
use veoveo_mcp_contract::{GatewayInternalIdentity, GatewayInternalTokenVerifier};
use veoveo_types::{ScopeDefinition, ScopeName};

use crate::contract::TimeScope;

#[derive(Clone)]
pub(crate) struct ForwardedBearer;
#[derive(Clone)]
pub(super) struct InternalAuthState {
    pub verifier: GatewayInternalTokenVerifier,
}
#[derive(Clone)]
pub(super) struct AdminAuthState {
    pub required_scope: ScopeName,
}

pub(super) async fn authenticate_internal(
    State(state): State<InternalAuthState>,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
    let token = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(bearer_token)
        .map(ToOwned::to_owned);
    let identity = token
        .as_deref()
        .and_then(|token| state.verifier.verify(token).ok());
    let Some(identity) = identity else {
        tracing::warn!("rejected unsigned or invalid Time request");
        return (StatusCode::UNAUTHORIZED, "invalid gateway authorization").into_response();
    };
    request.extensions_mut().insert(ForwardedBearer);
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
        .is_some_and(|identity| identity.actor.scopes.contains(&state.required_scope));
    if !allowed {
        return (StatusCode::FORBIDDEN, "time administrative scope required").into_response();
    }
    next.run(request).await
}

pub(crate) fn require_scope(
    grants: &std::collections::BTreeSet<ScopeName>,
    required: TimeScope,
) -> Result<(), rmcp::ErrorData> {
    grants
        .contains(required.name())
        .then_some(())
        .ok_or_else(|| {
            rmcp::ErrorData::invalid_request(
                format!(
                    "You don't have permission to make this request. Missing scope `{required}`."
                ),
                None,
            )
        })
}

fn bearer_token(header: &str) -> Option<&str> {
    let (scheme, token) = header.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer")
        && !token.is_empty()
        && !token.chars().any(char::is_whitespace))
    .then_some(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn typed_scope_admission_preserves_independent_grants_and_denials() {
        let mut grants = BTreeSet::from([
            ScopeName::new("unrelated-server:custom").unwrap(),
            TimeScope::Read.into(),
            TimeScope::Schedule.into(),
        ]);
        for scope in TimeScope::ALL {
            assert_eq!(
                require_scope(&grants, *scope).is_ok(),
                matches!(scope, TimeScope::Read | TimeScope::Schedule)
            );
        }
        grants.remove(TimeScope::Schedule.name());
        assert!(require_scope(&grants, TimeScope::Schedule).is_err());
        assert!(require_scope(&BTreeSet::new(), TimeScope::Read).is_err());
    }
}
