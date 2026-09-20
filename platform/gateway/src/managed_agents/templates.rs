use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use veoveo_mcp_contract::{Principal, WorkContextId, agent_management as wire};

use crate::GatewayCatalog;
pub use veoveo_mcp_contract::agent_management::runtime_template_revision;

#[derive(Clone, Debug, Default)]
pub struct ManagedTemplateCatalog {
    templates: BTreeMap<wire::AgentTemplateId, wire::RuntimeTemplate>,
}

impl ManagedTemplateCatalog {
    pub fn from_env(catalog: &GatewayCatalog) -> Result<Self> {
        let value = std::env::var("VEOVEO_AGENT_TEMPLATES").unwrap_or_else(|_| "[]".into());
        Self::from_json(&value, catalog)
    }

    pub fn from_json(value: &str, catalog: &GatewayCatalog) -> Result<Self> {
        ensure!(
            value.len() <= 256 * 1024,
            "managed template configuration exceeds 256 KiB"
        );
        let templates: Vec<wire::RuntimeTemplate> =
            serde_json::from_str(value).context("invalid managed templates")?;
        ensure!(templates.len() <= 64, "too many managed templates");
        let mut result = Self::default();
        for template in templates {
            template.validate(catalog.control_plane())?;
            ensure!(
                result
                    .templates
                    .insert(template.id.clone(), template)
                    .is_none(),
                "duplicate managed template"
            );
        }
        Ok(result)
    }

    pub fn get(&self, id: &wire::AgentTemplateId) -> Option<&wire::RuntimeTemplate> {
        self.templates.get(id)
    }

    pub fn choices(
        &self,
        principal: &Principal,
        context: &WorkContextId,
    ) -> Vec<wire::TemplateChoice> {
        self.templates
            .values()
            .filter(|t| t.permits(principal, context))
            .map(|t| wire::TemplateChoice {
                roles: t.roles.iter().cloned().collect(),
                id: t.id.clone(),
                revision: runtime_template_revision(t),
                name: t.name.clone(),
                models: t.models.iter().cloned().collect(),
                parameters: t
                    .parameters
                    .iter()
                    .map(|(name, p)| wire::ParameterChoice {
                        name: name.clone(),
                        label: p.label.clone(),
                        shape: p.shape.clone(),
                    })
                    .collect(),
                tools: t.tools.iter().cloned().collect(),
                resource_subscriptions: t.resource_subscriptions.iter().cloned().collect(),
                scopes: t.scopes.iter().cloned().collect(),
                membership: t.membership,
                storage_gib: t.workload.storage_gib,
            })
            .collect()
    }
}
