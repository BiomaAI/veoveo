use crate::contract::ViewScope;
use veoveo_mcp_contract::GatewayInternalIdentity;
#[cfg(test)]
use veoveo_types::ScopeDefinition;

pub(crate) fn has_scope(identity: &GatewayInternalIdentity, required: ViewScope) -> bool {
    super::setup::SERVER_SETUP.has_scope(&identity.actor.scopes, required)
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
