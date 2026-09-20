use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use veoveo_mcp_contract::{GatewayControlPlane, agent_management as wire};
use veoveo_platform_store::agent_management::{
    AgentExecution, AgentTemplateParameter, instances::ManagedAgentReconciliation,
};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub namespace: String,
    pub gateway_url: String,
    pub gateway_transport_url: String,
    pub store_endpoint: String,
    pub store_namespace: String,
    pub store_database: String,
    pub templates: Vec<wire::RuntimeTemplate>,
    pub models: Vec<wire::ModelConnection>,
}

impl Config {
    pub fn load(path: &Path, control: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        ensure!(
            bytes.len() <= 512 * 1024,
            "manager configuration exceeds 512 KiB"
        );
        let config: Self = serde_json::from_slice(&bytes)?;
        let control: GatewayControlPlane = serde_json::from_slice(&std::fs::read(control)?)?;
        wire::validate_model_connections(&config.models, &control)?;
        let mut ids = BTreeSet::new();
        ensure!(
            !config.templates.is_empty() && config.templates.len() <= 32,
            "manager requires one to 32 templates"
        );
        for template in &config.templates {
            template.validate(&control)?;
            ensure!(
                template.workload.namespace == config.namespace && ids.insert(&template.id),
                "template namespace or identity mismatch"
            );
        }
        for origin in [&config.gateway_url, &config.gateway_transport_url] {
            let url = reqwest::Url::parse(origin)?;
            ensure!(
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.path() == "/"
                    && url.query().is_none()
                    && url.fragment().is_none(),
                "invalid gateway origin"
            );
        }
        Ok(config)
    }

    pub fn execution<'a>(
        &'a self,
        snapshot: &ManagedAgentReconciliation,
    ) -> Result<(&'a wire::RuntimeTemplate, &'a wire::ModelConnection)> {
        let instance = &snapshot.instance;
        let content = &snapshot.revision.content;
        let AgentExecution::Managed {
            template,
            template_revision,
            parameters,
            resource_subscriptions,
        } = &content.execution
        else {
            anyhow::bail!("instance requires a managed revision");
        };
        let template = self
            .templates
            .iter()
            .find(|t| t.id.as_str() == template)
            .context("approved runtime template is unavailable")?;
        ensure!(
            wire::runtime_template_revision(template).as_str()
                == format!("sha256:{template_revision}"),
            "runtime template changed"
        );
        let context = veoveo_mcp_contract::WorkContextId::new(snapshot.context_key.clone())?;
        ensure!(
            template.tenant.as_str() == snapshot.tenant_key
                && template.work_contexts.contains(&context)
                && instance.resources.namespace == template.workload.namespace
                && instance.resources.image == template.workload.image
                && instance.resources.template_config_map == template.workload.config_map
                && instance.resources.storage_gib == template.workload.storage_gib
                && instance.identity.profile == template.profile.as_str()
                && instance
                    .identity
                    .scopes
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    == template.scopes.iter().map(ToString::to_string).collect(),
            "instance exceeds the current template"
        );
        let parameters = parameters
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    match value {
                        AgentTemplateParameter::Text(v) => wire::TemplateParameter::Text(v.clone()),
                        AgentTemplateParameter::Integer(v) => wire::TemplateParameter::Integer(*v),
                        AgentTemplateParameter::Boolean(v) => wire::TemplateParameter::Boolean(*v),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        ensure!(
            template.accepts_parameters(&parameters)
                && content.tools.iter().all(|tool| template
                    .tools
                    .iter()
                    .any(|allowed| allowed.as_str() == tool))
                && resource_subscriptions.iter().all(|uri| template
                    .resource_subscriptions
                    .iter()
                    .any(|allowed| allowed.as_str() == uri)),
            "revision exceeds current capability limits"
        );
        let model = self
            .models
            .iter()
            .find(|model| model.id.as_str() == content.model.id)
            .context("approved model is unavailable")?;
        ensure!(
            template.models.contains(&model.id)
                && model.tenant == template.tenant
                && model.work_contexts.contains(&context)
                && model.required_scopes.is_subset(&template.scopes)
                && model.revision().as_str() == format!("sha256:{}", content.model.revision)
                && template
                    .workload
                    .model_secrets
                    .iter()
                    .any(|binding| binding.reference == model.api_key)
                && model.admits(&wire::Budgets {
                    max_output_tokens: content.budgets.max_output_tokens,
                    max_completion_calls: content.budgets.max_completion_calls,
                    max_tool_calls: content.budgets.max_tool_calls,
                    deadline_seconds: content.budgets.deadline_seconds
                }),
            "model revision or authority changed"
        );
        Ok((template, model))
    }
}
