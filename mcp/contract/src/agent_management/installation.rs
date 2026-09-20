//! Installation-owned model connections and executable configuration digests.
use super::{self as wire, RuntimeTemplate};
use crate::{
    GatewayControlPlane, Principal, ScopeName, SecretPurpose, SecretReferenceId, Sha256Digest,
    TenantId, WorkContextId,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConnection {
    pub id: wire::AgentModelId,
    pub name: String,
    pub provider: String,
    pub tenant: TenantId,
    pub work_contexts: Vec<WorkContextId>,
    #[serde(default)]
    pub required_scopes: BTreeSet<ScopeName>,
    pub base_url: String,
    pub model: String,
    pub api_key: SecretReferenceId,
    pub limits: wire::Budgets,
}

impl ModelConnection {
    pub fn permits(&self, principal: &Principal, context: &WorkContextId) -> bool {
        principal.tenant.as_ref() == Some(&self.tenant)
            && self.work_contexts.contains(context)
            && self.required_scopes.is_subset(&principal.scopes)
    }
    pub fn revision(&self) -> Sha256Digest {
        Sha256Digest::from_hex(hex::encode(Sha256::digest(
            serde_json::to_vec(&(&self.base_url, &self.model, &self.api_key, &self.limits))
                .expect("typed model connection"),
        )))
        .expect("SHA256 digest")
    }
    pub fn public(&self) -> wire::ModelChoice {
        wire::ModelChoice {
            reference: wire::ModelReference {
                id: self.id.clone(),
                revision: self.revision(),
            },
            name: self.name.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            limits: self.limits.clone(),
        }
    }
    pub fn admits(&self, budgets: &wire::Budgets) -> bool {
        budgets.max_output_tokens > 0
            && budgets.max_output_tokens <= self.limits.max_output_tokens
            && budgets.max_completion_calls > 0
            && budgets.max_completion_calls <= self.limits.max_completion_calls
            && budgets.max_tool_calls <= self.limits.max_tool_calls
            && budgets.deadline_seconds > 0
            && budgets.deadline_seconds <= self.limits.deadline_seconds
    }
}

pub fn validate_model_connections(
    models: &[ModelConnection],
    catalog: &GatewayControlPlane,
) -> Result<()> {
    ensure!(
        models.len() <= 64,
        "at most 64 model connections are admitted"
    );
    let mut ids = BTreeSet::new();
    for model in models {
        ensure!(ids.insert(&model.id), "duplicate model connection");
        for (field, value, max) in [
            ("name", &model.name, 200),
            ("provider", &model.provider, 200),
            ("model", &model.model, 256),
        ] {
            ensure!(
                !value.trim().is_empty() && value.len() <= max && !value.contains('\0'),
                "invalid model {field}"
            );
        }
        let url = url::Url::parse(&model.base_url).context("invalid model destination")?;
        ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "model destination must be HTTP(S) without embedded credentials, query or fragment"
        );
        ensure!(
            !model.work_contexts.is_empty() && model.work_contexts.len() <= 64,
            "model requires bounded Work Context admission"
        );
        for context in &model.work_contexts {
            ensure!(
                catalog
                    .work_contexts
                    .iter()
                    .any(|c| c.id == *context && c.tenant == model.tenant),
                "model context does not belong to its tenant"
            );
        }
        ensure!(
            catalog
                .secrets
                .iter()
                .find(|s| s.id == model.api_key)
                .is_some_and(|s| s.purpose == SecretPurpose::ProviderApiKey),
            "model requires registered provider_api_key reference"
        );
        let limits = &model.limits;
        ensure!(
            (1..=8192).contains(&limits.max_output_tokens)
                && (1..=32).contains(&limits.max_completion_calls)
                && limits.max_tool_calls <= 64
                && (1..=900).contains(&limits.deadline_seconds),
            "invalid model execution ceilings"
        );
    }
    Ok(())
}

/// The typed struct's field order and sorted maps define this repository profile.
/// This does not claim RFC 8785 canonicalization of arbitrary JSON.
pub fn runtime_template_revision(template: &RuntimeTemplate) -> Sha256Digest {
    Sha256Digest::from_hex(hex::encode(Sha256::digest(
        serde_json::to_vec(template).expect("typed runtime template"),
    )))
    .expect("SHA256")
}

/// Immutable ConfigMap data uses sorted string keys and compact UTF-8 JSON.
pub fn runtime_config_revision(data: &BTreeMap<String, String>) -> Sha256Digest {
    Sha256Digest::from_hex(hex::encode(Sha256::digest(
        serde_json::to_vec(data).expect("string map"),
    )))
    .expect("SHA256")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_model_identity_survives_shared_configuration_extraction() {
        let model: ModelConnection = serde_json::from_value(serde_json::json!({
            "id":"approved", "name":"Approved model", "provider":"Fixture", "tenant":"test",
            "work_contexts":["shared"], "base_url":"https://provider.test/v1", "model":"model",
            "api_key":"fixture-secret", "limits":{"maxOutputTokens":128,"maxCompletionCalls":4,"maxToolCalls":8,"deadlineSeconds":120}
        })).unwrap();
        assert_eq!(
            model.revision().hex(),
            "d72e1cf67be43b0a8531631684d5f4bb0283af603207d1845e15872926de6811"
        );
    }

    #[test]
    fn agent_runtime_config_digest_binds_sorted_immutable_data() {
        let mut data = BTreeMap::from([("z".into(), "last".into()), ("a".into(), "first".into())]);
        let original = runtime_config_revision(&data);
        assert_eq!(
            original.hex(),
            "af3bf072ea841a14c20d546622a7edd6ecb1b6a59c12f49cef627eac17416041"
        );
        data.insert("z".into(), "changed".into());
        assert_ne!(runtime_config_revision(&data), original);
    }
}
