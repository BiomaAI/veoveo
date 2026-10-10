use std::collections::BTreeMap;

use crate::contract::authoring as wire;
use anyhow::{Context, Result, ensure};
use veoveo_mcp_contract::Principal;
use veoveo_types::WorkContextId;

pub use crate::contract::authoring::runtime_template_revision;
use veoveo_mcp_gateway::GatewayCatalog;

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
        let facts = crate::gateway::installation::installation_facts(catalog)?;
        let mut result = Self::default();
        for template in templates {
            template.validate(&facts)?;
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
            .filter(|t| {
                t.permits(
                    crate::gateway::installation::caller_facts(principal),
                    context,
                )
            })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_loading_rejects_missing_profile_scopes_without_widening_grants() {
        let control =
            serde_json::from_str(include_str!("../../../../examples/bioma/gateway.json")).unwrap();
        let catalog =
            GatewayCatalog::from_control_plane(control, crate::catalog_fixture::binding()).unwrap();
        let capture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../manager/testdata/rendered-installation.json"
        ))
        .unwrap();
        let manager: serde_json::Value =
            serde_json::from_str(capture["managerData"]["manager.json"].as_str().unwrap()).unwrap();
        let template: wire::RuntimeTemplate =
            serde_json::from_value(manager["templates"][0].clone()).unwrap();
        let grants = template.scopes.clone();
        assert_eq!(grants.len(), 5);
        let admitted = ManagedTemplateCatalog::from_json(
            &serde_json::to_string(&[&template]).unwrap(),
            &catalog,
        )
        .unwrap();
        assert_eq!(admitted.get(&template.id).unwrap().scopes, grants);
        let mut missing = template;
        missing
            .scopes
            .retain(|scope| scope.as_str() == "operator:use");
        let error = ManagedTemplateCatalog::from_json(
            &serde_json::to_string(&[&missing]).unwrap(),
            &catalog,
        )
        .unwrap_err();
        assert!(error.to_string().contains("requires missing scope"));
        assert_eq!(missing.scopes.len(), 1);
    }
}
