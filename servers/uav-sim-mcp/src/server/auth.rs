use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::IntoResponse,
};
use veoveo_mcp_contract::{GatewayInternalIdentity, GatewayInternalTokenVerifier};
use veoveo_types::ScopeDefinition;

use crate::contract::UavScope;

pub(super) fn identity_has_scope(identity: &GatewayInternalIdentity, required: UavScope) -> bool {
    identity.actor.scopes.contains(required.name())
}

pub(super) fn require_scope(
    identity: &GatewayInternalIdentity,
    required: UavScope,
) -> Result<(), rmcp::ErrorData> {
    identity_has_scope(identity, required)
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

pub(super) fn require_any_scope(
    identity: &GatewayInternalIdentity,
    required: &[UavScope],
) -> Result<(), rmcp::ErrorData> {
    required
        .iter()
        .any(|scope| identity_has_scope(identity, *scope))
        .then_some(())
        .ok_or_else(|| {
            rmcp::ErrorData::invalid_request(
                format!(
                    "You don't have permission to make this request. It needs one of these scopes: {}.",
                    required.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
                ),
                None,
            )
        })
}

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
            tracing::warn!(%error, "rejected UAV simulation MCP request");
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

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_types::ScopeName;

    #[test]
    fn typed_permissions_preserve_each_grant_combination_and_external_scopes() {
        let mut identity =
            crate::server::test_support::identity("tenant", "operations", "pilot", &[]);
        for bits in 0..(1 << UavScope::ALL.len()) {
            identity.actor.scopes = [ScopeName::new("other:read").unwrap()].into();
            for (index, scope) in UavScope::ALL.iter().enumerate() {
                if bits & (1 << index) != 0 {
                    identity.actor.scopes.insert((*scope).into());
                }
            }
            for (index, scope) in UavScope::ALL.iter().enumerate() {
                let allowed = bits & (1 << index) != 0;
                assert_eq!(identity_has_scope(&identity, *scope), allowed);
                assert_eq!(require_scope(&identity, *scope).is_ok(), allowed);
            }
            assert_eq!(
                require_any_scope(&identity, &[UavScope::Control, UavScope::Admin]).is_ok(),
                bits & 0b0110 != 0
            );
            assert!(require_any_scope(&identity, &[]).is_err());
        }
        identity.actor.scopes.clear();
        for scope in UavScope::ALL {
            assert!(require_scope(&identity, *scope).is_err());
        }
    }
}
