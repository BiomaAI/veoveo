use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{Principal, Sha256Digest, WorkContextId, agent_management as wire};

use crate::GatewayCatalog;

#[derive(Clone, Debug, Default)]
pub struct ManagedTemplateCatalog {
    templates: BTreeMap<wire::AgentTemplateId, wire::RuntimeTemplate>,
}

pub fn runtime_template_revision(template: &wire::RuntimeTemplate) -> Sha256Digest {
    Sha256Digest::from_hex(hex::encode(Sha256::digest(
        serde_json::to_vec(template).expect("typed runtime template"),
    )))
    .expect("SHA256")
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
            validate(&template, catalog)?;
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
            .filter(|t| Self::permits(t, principal, context))
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

    pub fn permits(
        template: &wire::RuntimeTemplate,
        principal: &Principal,
        context: &WorkContextId,
    ) -> bool {
        principal.tenant.as_ref() == Some(&template.tenant)
            && template.work_contexts.contains(context)
            && template
                .required_deployer_scopes
                .is_subset(&principal.scopes)
    }

    pub fn parameters(
        template: &wire::RuntimeTemplate,
        parameters: &BTreeMap<String, wire::TemplateParameter>,
    ) -> bool {
        parameters.len() == template.parameters.len()
            && template.parameters.iter().all(|(key, spec)| {
                match (&spec.shape, parameters.get(key)) {
                    (
                        wire::ParameterShape::Identifier { max_length },
                        Some(wire::TemplateParameter::Text(value)),
                    ) => {
                        !value.is_empty()
                            && value.len() <= *max_length as usize
                            && value
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                    }
                    (
                        wire::ParameterShape::Choice { values },
                        Some(wire::TemplateParameter::Text(value)),
                    ) => values.contains(value),
                    (
                        wire::ParameterShape::Integer { minimum, maximum },
                        Some(wire::TemplateParameter::Integer(value)),
                    ) => (*minimum..=*maximum).contains(value),
                    (wire::ParameterShape::Boolean, Some(wire::TemplateParameter::Boolean(_))) => {
                        true
                    }
                    _ => false,
                }
            })
    }
}

fn validate(template: &wire::RuntimeTemplate, catalog: &GatewayCatalog) -> Result<()> {
    ensure!(
        !template.name.trim().is_empty() && template.name.len() <= 200,
        "invalid template name"
    );
    ensure!(
        !template.work_contexts.is_empty() && template.work_contexts.len() <= 64,
        "template requires bounded contexts"
    );
    for context in &template.work_contexts {
        ensure!(
            catalog
                .work_context(context)
                .is_some_and(|c| c.tenant == template.tenant),
            "template context belongs to another tenant"
        );
    }
    ensure!(
        catalog.profile(&template.profile).is_some(),
        "template profile is not installed"
    );
    ensure!(
        !template.scopes.is_empty() && template.scopes.len() <= 64 && template.roles.len() <= 16,
        "invalid automated authority"
    );
    ensure!(
        !template.models.is_empty()
            && template.models.len() <= 64
            && template.tools.len() <= 128
            && template.resource_subscriptions.len() <= 128,
        "invalid capability bounds"
    );
    let w = &template.workload;
    for name in [
        &w.namespace,
        &w.config_map,
        &w.database_secret,
        &w.storage_class,
    ] {
        kubernetes_name(name)?;
    }
    ensure!(
        (1..=1024).contains(&w.storage_gib)
            && (100..=16000).contains(&w.cpu_millis)
            && (256..=65536).contains(&w.memory_mib),
        "invalid managed resource bounds"
    );
    let (repository, digest) = w
        .image
        .rsplit_once("@sha256:")
        .context("template image must use a digest")?;
    ensure!(
        !repository.is_empty() && w.image.len() <= 512 && !w.image.contains(char::is_whitespace),
        "invalid template image"
    );
    Sha256Digest::from_hex(digest).context("invalid template image digest")?;
    let mut secrets = BTreeSet::new();
    ensure!(
        !w.model_secrets.is_empty() && w.model_secrets.len() <= 64,
        "invalid model credential bindings"
    );
    for binding in &w.model_secrets {
        ensure!(
            secrets.insert(&binding.reference)
                && catalog.secret_reference(&binding.reference).is_some_and(
                    |s| s.purpose == veoveo_mcp_contract::SecretPurpose::ProviderApiKey
                ),
            "template requires unique approved model credentials"
        );
        kubernetes_name(&binding.secret)?;
        ensure!(
            !binding.key.is_empty()
                && binding.key.len() <= 128
                && binding
                    .key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.'),
            "invalid model credential key"
        );
    }
    ensure!(
        template.parameters.len() <= 32,
        "too many template parameters"
    );
    let mut environment = BTreeSet::new();
    for (name, parameter) in &template.parameters {
        wire::AgentTemplateId::new(name.clone()).context("invalid parameter name")?;
        let variable = &parameter.environment_variable;
        // Author-controlled values can only enter the dedicated parameter namespace.
        ensure!(
            variable.starts_with("VEOVEO_PARAM_")
                && variable.len() <= 128
                && variable
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                && environment.insert(variable),
            "invalid or duplicated parameter environment binding"
        );
        ensure!(
            !parameter.label.trim().is_empty() && parameter.label.len() <= 200,
            "invalid parameter label"
        );
        match &parameter.shape {
            wire::ParameterShape::Identifier { max_length } => {
                ensure!((1..=128).contains(max_length), "invalid parameter length")
            }
            wire::ParameterShape::Choice { values } => ensure!(
                !values.is_empty()
                    && values.len() <= 128
                    && values.iter().all(|v| !v.is_empty()
                        && v.len() <= 128
                        && v.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')),
                "invalid parameter choices"
            ),
            wire::ParameterShape::Integer { minimum, maximum } => {
                ensure!(minimum <= maximum, "invalid parameter range")
            }
            wire::ParameterShape::Boolean => {}
        }
    }
    Ok(())
}

fn kubernetes_name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 63
            && !value.starts_with('-')
            && !value.ends_with('-')
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "invalid Kubernetes resource name"
    );
    Ok(())
}
