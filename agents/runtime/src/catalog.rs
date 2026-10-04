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
        control.profiles.iter().map(|profile| profile.id.clone()),
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
            assert!(facts.profile_installed(&profile.id));
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
}
