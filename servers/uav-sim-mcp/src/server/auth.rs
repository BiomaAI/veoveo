use veoveo_mcp_contract::GatewayInternalIdentity;

use crate::contract::UavScope;

pub(super) fn identity_has_scope(identity: &GatewayInternalIdentity, required: UavScope) -> bool {
    super::setup::SERVER_SETUP.has_scope(&identity.actor.scopes, required)
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
