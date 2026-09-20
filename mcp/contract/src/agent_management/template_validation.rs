use super as wire;
use crate::{GatewayControlPlane, Principal, Sha256Digest, WorkContextId};
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

impl wire::RuntimeTemplate {
    pub fn validate(&self, catalog: &GatewayControlPlane) -> Result<()> {
        let template = self;
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
                    .work_contexts
                    .iter()
                    .find(|c| c.id == *context)
                    .is_some_and(|c| c.tenant == template.tenant),
                "template context belongs to another tenant"
            );
        }
        ensure!(
            catalog.profiles.iter().any(|p| p.id == template.profile),
            "template profile is not installed"
        );
        ensure!(
            !template.scopes.is_empty()
                && template.scopes.len() <= 64
                && template.roles.len() <= 16,
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
            !repository.is_empty()
                && w.image.len() <= 512
                && !w.image.contains(char::is_whitespace),
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
                    && catalog
                        .secrets
                        .iter()
                        .find(|s| s.id == binding.reference)
                        .is_some_and(|s| s.purpose == crate::SecretPurpose::ProviderApiKey),
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
                    ensure!(
                        minimum <= maximum
                            && *minimum >= -9_007_199_254_740_991
                            && *maximum <= 9_007_199_254_740_991,
                        "invalid parameter range"
                    )
                }
                wire::ParameterShape::Boolean => {}
            }
        }
        Ok(())
    }

    pub fn permits(&self, principal: &Principal, context: &WorkContextId) -> bool {
        principal.tenant.as_ref() == Some(&self.tenant)
            && self.work_contexts.contains(context)
            && self.required_deployer_scopes.is_subset(&principal.scopes)
    }

    pub fn accepts_parameters(
        &self,
        parameters: &BTreeMap<String, wire::TemplateParameter>,
    ) -> bool {
        parameters.len() == self.parameters.len()
            && self
                .parameters
                .iter()
                .all(|(key, spec)| match (&spec.shape, parameters.get(key)) {
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
                })
    }
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
