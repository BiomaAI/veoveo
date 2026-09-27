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
    pub required_scope: veoveo_types::ScopeName,
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
        .is_some_and(|identity| identity.actor.scopes.contains(&state.required_scope));
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

pub(crate) fn require_scope(
    grants: &std::collections::BTreeSet<veoveo_types::ScopeName>,
    required: crate::contract::MapScope,
) -> Result<(), rmcp::ErrorData> {
    use veoveo_types::ScopeDefinition;
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

#[cfg(test)]
mod tests {
    use super::require_scope;
    use crate::contract::MapScope;
    use std::collections::BTreeSet;
    use veoveo_types::ScopeName;

    #[test]
    fn typed_admission_preserves_other_domains_and_revocation() {
        let mut grants = BTreeSet::from([
            ScopeName::new("another-server:custom").unwrap(),
            MapScope::FeatureRead.into(),
        ]);
        assert!(require_scope(&grants, MapScope::FeatureRead).is_ok());
        assert!(require_scope(&grants, MapScope::FeatureWrite).is_err());
        grants.remove(&MapScope::FeatureRead.into());
        assert!(require_scope(&grants, MapScope::FeatureRead).is_err());
        grants.insert(MapScope::Admin.into());
        assert!(require_scope(&grants, MapScope::Admin).is_ok());
        assert!(require_scope(&grants, MapScope::FeatureRead).is_err());
    }
}
