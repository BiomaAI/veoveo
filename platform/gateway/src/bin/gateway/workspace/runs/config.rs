//! Installation-owned model admission. No browser may choose these destinations.
use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;
use veoveo_mcp_contract::{
    GatewayToolName, SecretPurpose, SecretReferenceId, TenantId, WorkContextId, workspace as wire,
};
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayCatalog};
use veoveo_platform_store::workspace::WorkspaceAgentAdmission;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Definition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub provider: String,
    pub tenant: TenantId,
    pub work_contexts: Vec<WorkContextId>,
    pub model: Model,
    pub instructions: String,
    pub tools: Vec<GatewayToolName>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Model {
    pub base_url: String,
    pub name: String,
    pub api_key: SecretReferenceId,
    pub max_output_tokens: u32,
}

impl Definition {
    pub fn permits(&self, subject: &AuthenticatedSubject) -> bool {
        self.tenant == subject.authority.tenant
            && self.work_contexts.contains(&subject.authority.work_context)
    }
    pub fn digest(&self) -> String {
        hex::encode(Sha256::digest(
            serde_json::to_vec(self).expect("typed model definition"),
        ))
    }
    pub fn admission(&self) -> WorkspaceAgentAdmission {
        WorkspaceAgentAdmission {
            definition: self.id.clone(),
            definition_digest: self.digest(),
            display_name: self.name.clone(),
            provider: self.provider.clone(),
            model: self.model.name.clone(),
        }
    }
    pub fn public(&self) -> wire::AgentDefinition {
        wire::AgentDefinition {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            provider: self.provider.clone(),
            model: self.model.name.clone(),
            tools: self.tools.clone(),
        }
    }
}

pub(super) fn from_env(catalog: &GatewayCatalog) -> Result<Vec<Definition>> {
    let source = std::env::var("VEOVEO_WORKSPACE_AGENTS").unwrap_or_else(|_| "[]".into());
    ensure!(
        source.len() <= 128 * 1024,
        "Workspace agent configuration exceeds 128 KiB"
    );
    let definitions: Vec<Definition> =
        serde_json::from_str(&source).context("invalid VEOVEO_WORKSPACE_AGENTS")?;
    validate(&definitions, catalog)?;
    Ok(definitions)
}

pub(super) fn validate(definitions: &[Definition], catalog: &GatewayCatalog) -> Result<()> {
    ensure!(
        definitions.len() <= 64,
        "Workspace admits at most 64 configured agent definitions"
    );
    let mut ids = BTreeSet::new();
    for definition in definitions {
        ensure!(
            !definition.id.is_empty()
                && definition.id.len() <= 128
                && definition.id.bytes().all(|b| b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || b == b'-'
                    || b == b'_'),
            "Workspace agent id must use lowercase alphanumerics, hyphens or underscores"
        );
        ensure!(ids.insert(&definition.id), "duplicate Workspace agent id");
        ensure!(
            definition.tools.len() <= 64
                && definition.tools.iter().collect::<BTreeSet<_>>().len() == definition.tools.len(),
            "Workspace agent requires at most 64 distinct tool names"
        );
        for (name, value, max) in [
            ("name", definition.name.as_str(), 200),
            ("description", definition.description.as_str(), 2000),
            ("provider", definition.provider.as_str(), 200),
            ("model", definition.model.name.as_str(), 256),
            ("instructions", definition.instructions.as_str(), 16384),
        ] {
            ensure!(
                !value.trim().is_empty() && value.len() <= max && !value.contains('\0'),
                "invalid Workspace agent {name}"
            );
        }
        let url = Url::parse(&definition.model.base_url).context("invalid Workspace model URL")?;
        ensure!(
            matches!(url.scheme(), "https" | "http")
                && url.host_str().is_some()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.username().is_empty()
                && url.password().is_none(),
            "Workspace model URL must be HTTP(S) without embedded credentials, query or fragment"
        );
        ensure!(
            (1..=8192).contains(&definition.model.max_output_tokens),
            "Workspace model output budget must be 1..=8192"
        );
        ensure!(
            !definition.work_contexts.is_empty() && definition.work_contexts.len() <= 64,
            "Workspace agent requires bounded Work Context admission"
        );
        for context in &definition.work_contexts {
            ensure!(
                catalog
                    .control_plane()
                    .work_contexts
                    .iter()
                    .any(|value| value.id == *context && value.tenant == definition.tenant),
                "Workspace agent Work Context must belong to its configured tenant"
            );
        }
        let secret = catalog
            .secret_reference(&definition.model.api_key)
            .context("Workspace model secret reference is not registered")?;
        ensure!(
            secret.purpose == SecretPurpose::ProviderApiKey,
            "Workspace model key requires provider_api_key purpose"
        );
    }
    Ok(())
}
