use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::IntoResponse,
};
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_types::ScopeName;

use crate::contract::TimeScope;

#[derive(Clone)]
pub(super) struct AdminAuthState {
    pub required_scope: ScopeName,
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
    crate::mcp::SERVER_SETUP
        .has_scope(grants, required)
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
    use super::*;
    use std::collections::BTreeSet;
    use veoveo_types::ScopeDefinition;

    #[test]
    fn typed_scope_admission_preserves_independent_grants_and_denials() {
        let mut grants = BTreeSet::from([
            ScopeName::parse("unrelated-server:custom").unwrap(),
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
