//! Agent installation facts admitted by the shared catalog registry.
use anyhow::Result;
use veoveo_gateway_contract::CatalogRegistry;
use veoveo_mcp_contract::{GatewayControlPlane, Principal};

use crate::contract::authoring::{CallerFacts, InstallationFacts};

/// Validate the complete catalog before projecting owner facts from one snapshot.
pub fn installation_facts(
    control: &GatewayControlPlane,
    registry: &CatalogRegistry,
) -> Result<InstallationFacts> {
    control.validate(registry)?;
    InstallationFacts::new(
        control
            .work_contexts
            .iter()
            .map(|context| (context.id.clone(), context.tenant.clone())),
        control.profiles.iter().map(|profile| {
            (
                profile.id.clone(),
                profile.required_scopes.iter().cloned().collect(),
            )
        }),
        control
            .secrets
            .iter()
            .map(|secret| (secret.id.clone(), secret.purpose)),
    )
}

/// Borrow admitted caller facts without replacing service authorization.
pub fn caller_facts(principal: &Principal) -> CallerFacts<'_> {
    CallerFacts {
        tenant: principal.tenant.as_ref(),
        scopes: &principal.scopes,
    }
}

#[cfg(all(test, feature = "gateway"))]
mod tests {
    use super::*;

    #[test]
    fn installation_projection_requires_the_complete_admitted_catalog() {
        let mut control: GatewayControlPlane =
            serde_json::from_str(include_str!("../../../configs/gateway.smoke.json")).unwrap();
        let registry = crate::catalog_fixture::registry();
        let facts = installation_facts(&control, &registry).unwrap();
        for context in &control.work_contexts {
            assert_eq!(facts.context_tenant(&context.id), Some(&context.tenant));
        }
        for profile in &control.profiles {
            let required_scopes: std::collections::BTreeSet<_> =
                profile.required_scopes.iter().cloned().collect();
            assert_eq!(
                facts.profile_required_scopes(&profile.id),
                Some(&required_scopes)
            );
        }
        for secret in &control.secrets {
            assert_eq!(facts.secret_purpose(&secret.id), Some(&secret.purpose));
        }
        control.extensions.insert(
            "unregistered-owner".into(),
            serde_json::json!({"profiles": []}),
        );
        assert!(installation_facts(&control, &registry).is_err());
    }

    #[test]
    fn installation_projection_preserves_agent_profile_required_scopes() {
        let control: GatewayControlPlane =
            serde_json::from_str(include_str!("../../../examples/bioma/gateway.json")).unwrap();
        let facts = installation_facts(&control, &crate::catalog_fixture::registry()).unwrap();
        let profile = control
            .profiles
            .iter()
            .find(|profile| profile.id.as_str() == "agent")
            .unwrap();
        assert_eq!(profile.required_scopes.len(), 5);
        let required_scopes: std::collections::BTreeSet<_> =
            profile.required_scopes.iter().cloned().collect();
        assert_eq!(
            facts.profile_required_scopes(&profile.id),
            Some(&required_scopes)
        );
    }
}
