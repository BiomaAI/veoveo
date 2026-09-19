//! Installation-owned destinations and authoring ceilings. Public DTOs omit secrets.
use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    ScopeName, SecretPurpose, SecretReferenceId, Sha256Digest, TenantId, WorkContextId,
    agent_management as wire,
};
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayCatalog};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelConnection {
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
    pub fn permits(&self, subject: &AuthenticatedSubject, context: &WorkContextId) -> bool {
        self.tenant == subject.authority.tenant
            && self.work_contexts.contains(context)
            && self.required_scopes.is_subset(&subject.principal.scopes)
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

pub(crate) fn from_env(catalog: &GatewayCatalog) -> Result<Vec<ModelConnection>> {
    let source = std::env::var("VEOVEO_AGENT_MODELS").unwrap_or_else(|_| "[]".into());
    ensure!(
        source.len() <= 128 * 1024,
        "agent model configuration exceeds 128 KiB"
    );
    let models: Vec<ModelConnection> =
        serde_json::from_str(&source).context("invalid VEOVEO_AGENT_MODELS")?;
    validate(&models, catalog)?;
    Ok(models)
}

pub(crate) fn validate(models: &[ModelConnection], catalog: &GatewayCatalog) -> Result<()> {
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
                    .control_plane()
                    .work_contexts
                    .iter()
                    .any(|c| c.id == *context && c.tenant == model.tenant),
                "model context does not belong to its tenant"
            );
        }
        ensure!(
            catalog
                .secret_reference(&model.api_key)
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
