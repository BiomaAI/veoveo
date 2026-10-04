//! Installation facts from one admitted revision; current liveness belongs to adapters.
use anyhow::{Result, ensure};
use std::collections::{BTreeMap, BTreeSet};
use veoveo_gateway_contract::{SecretPurpose, SecretReferenceId};
use veoveo_types::{GatewayProfileId, ScopeName, TenantId, WorkContextId};

/// Immutable relationships needed by authoring validation, without a gateway catalog.
#[derive(Clone, Debug)]
pub struct InstallationFacts {
    contexts: BTreeMap<WorkContextId, TenantId>,
    profiles: BTreeSet<GatewayProfileId>,
    secrets: BTreeMap<SecretReferenceId, SecretPurpose>,
}
impl InstallationFacts {
    pub fn new(
        contexts: impl IntoIterator<Item = (WorkContextId, TenantId)>,
        profiles: impl IntoIterator<Item = GatewayProfileId>,
        secrets: impl IntoIterator<Item = (SecretReferenceId, SecretPurpose)>,
    ) -> Result<Self> {
        let mut admitted_contexts = BTreeMap::new();
        for (id, tenant) in contexts {
            ensure!(
                admitted_contexts.insert(id, tenant).is_none(),
                "duplicate installation Work Context"
            );
        }
        let mut admitted_profiles = BTreeSet::new();
        for profile in profiles {
            ensure!(
                admitted_profiles.insert(profile),
                "duplicate installed profile"
            );
        }
        let mut admitted_secrets = BTreeMap::new();
        for (id, purpose) in secrets {
            ensure!(
                admitted_secrets.insert(id, purpose).is_none(),
                "duplicate installation secret reference"
            );
        }
        Ok(Self {
            contexts: admitted_contexts,
            profiles: admitted_profiles,
            secrets: admitted_secrets,
        })
    }
    pub fn context_tenant(&self, context: &WorkContextId) -> Option<&TenantId> {
        self.contexts.get(context)
    }
    pub fn profile_installed(&self, profile: &GatewayProfileId) -> bool {
        self.profiles.contains(profile)
    }
    pub fn secret_purpose(&self, reference: &SecretReferenceId) -> Option<&SecretPurpose> {
        self.secrets.get(reference)
    }
}

/// Borrowed caller authority facts; constructing these does not authenticate a caller.
#[derive(Clone, Copy, Debug)]
pub struct CallerFacts<'a> {
    pub tenant: Option<&'a TenantId>,
    pub scopes: &'a BTreeSet<ScopeName>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_installation_keys_never_choose_a_winner() {
        let context: WorkContextId = "shared".parse().unwrap();
        let tenant: TenantId = "tenant-a".parse().unwrap();
        assert!(
            InstallationFacts::new(
                [(context.clone(), tenant.clone()), (context, tenant)],
                [],
                []
            )
            .is_err()
        );
        let profile: GatewayProfileId = "author".parse().unwrap();
        assert!(InstallationFacts::new([], [profile.clone(), profile], []).is_err());
        let secret: SecretReferenceId = "provider".parse().unwrap();
        assert!(
            InstallationFacts::new(
                [],
                [],
                [
                    (secret.clone(), SecretPurpose::ProviderApiKey),
                    (secret, SecretPurpose::ProviderApiKey)
                ]
            )
            .is_err()
        );
    }
}
