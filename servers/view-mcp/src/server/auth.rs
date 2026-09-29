use crate::contract::ViewScope;
use axum::{
    extract::{Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::IntoResponse,
};
use veoveo_mcp_contract::{GatewayInternalIdentity, GatewayInternalTokenVerifier};
use veoveo_types::ScopeDefinition;

#[derive(Clone)]
pub(crate) struct ForwardedBearer(pub String);

#[derive(Clone)]
pub(super) struct InternalAuthState {
    pub verifier: GatewayInternalTokenVerifier,
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
    let (Some(token), Some(identity)) = (token, identity) else {
        tracing::warn!("rejected unsigned or invalid View request");
        return (StatusCode::UNAUTHORIZED, "invalid gateway authorization").into_response();
    };
    request.extensions_mut().insert(ForwardedBearer(token));
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

pub(crate) fn has_scope(identity: &GatewayInternalIdentity, required: ViewScope) -> bool {
    identity.actor.scopes.contains(required.name())
}

pub(crate) fn require_scope(
    identity: &GatewayInternalIdentity,
    required: ViewScope,
) -> Result<(), rmcp::ErrorData> {
    has_scope(identity, required).then_some(()).ok_or_else(|| {
        rmcp::ErrorData::invalid_request(
            format!("You don't have permission to make this request. Missing scope `{required}`."),
            None,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_and_task_permission_guard_requires_each_explicit_view_grant() {
        // This exercises grant selection after authentication, not JWT verification.
        let fixtures: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../../testing/fixtures/gateway-request-context.json"
        ))
        .unwrap();
        let mut wire = fixtures[0].clone();
        wire["request_context"] = serde_json::Value::Null;
        for (field, value) in [
            ("issuer", json!("https://issuer.test")),
            ("profile", json!("operator")),
            ("server", json!("view")),
            ("jwt_id", json!("view-scope-test")),
            ("issued_at", json!("2026-09-28T00:00:00Z")),
            ("not_before", json!("2026-09-28T00:00:00Z")),
            ("expires_at", json!("2026-09-28T01:00:00Z")),
        ] {
            wire[field] = value;
        }
        let baseline: GatewayInternalIdentity = serde_json::from_value(wire).unwrap();
        for required in ViewScope::ALL {
            assert!(require_scope(&baseline, *required).is_err());
        }
        for granted in ViewScope::ALL {
            let mut identity = baseline.clone();
            identity.actor.scopes.insert(granted.name().clone());
            for required in ViewScope::ALL {
                assert_eq!(has_scope(&identity, *required), granted == required);
                assert_eq!(
                    require_scope(&identity, *required).is_ok(),
                    granted == required
                );
            }
        }
    }
}
