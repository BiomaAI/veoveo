//! Pure catalog declaration and cross-reference admission for Recording.
use super::{RecordingCatalog, RecordingIngestResource, RecordingTarget};
use crate::RecordingAction;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_gateway_contract::{
    CatalogFacts, CatalogRegistryBuilder, CatalogSection, ProtectedResourceDescriptor,
};
use veoveo_types::{ExtensionError, ExtensionName};

pub const RECORDING_INGEST_SECTION: &str = "recording_ingest_resources";
pub const RECORDING_TARGET_GROUP: &str = "recording";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct RecordingCatalogSection(pub Vec<RecordingIngestResource>);
impl CatalogSection for RecordingCatalogSection {
    fn validate(&self, facts: &CatalogFacts) -> Result<(), ExtensionError> {
        RecordingCatalog::new(self.0.clone())
            .map_err(|error| ExtensionError::new(error.to_string()))?;
        for resource in &self.0 {
            let fail = |reason: String| {
                ExtensionError::new(format!(
                    "Recording ingest resource `{}`: {reason}",
                    resource.id
                ))
            };
            if !facts
                .authorization_servers
                .contains(&resource.authorization_server)
            {
                return Err(fail(format!(
                    "unknown authorization server `{}`",
                    resource.authorization_server
                )));
            }
            if !facts.policies.contains(&resource.policy_version) {
                return Err(fail(format!(
                    "unknown policy version `{}`",
                    resource.policy_version
                )));
            }
            for producer in &resource.producers {
                if !facts.tenants.contains(&producer.tenant) {
                    return Err(fail(format!(
                        "producer `{}` references unknown tenant `{}`",
                        producer.id, producer.tenant
                    )));
                }
                if let Some(label) = producer
                    .labels
                    .iter()
                    .find(|label| !facts.data_labels.contains(*label))
                {
                    return Err(fail(format!(
                        "producer `{}` references unknown data label `{label}`",
                        producer.id
                    )));
                }
                let client = facts
                    .oauth_clients
                    .iter()
                    .find(|client| client.id == producer.oauth_client)
                    .ok_or_else(|| {
                        fail(format!(
                            "producer `{}` references unknown OAuth client `{}`",
                            producer.id, producer.oauth_client
                        ))
                    })?;
                if client.authorization_server != resource.authorization_server
                    || !client
                        .allowed_resources
                        .contains(&resource.protected_resource)
                    || client.tenant.as_ref() != Some(&producer.tenant)
                    || !client.client_credentials
                    || !client.private_key_jwt
                {
                    return Err(fail(format!(
                        "producer `{}` OAuth client is not bound to the resource, tenant, and private_key_jwt client-credentials grant",
                        producer.id
                    )));
                }
            }
        }
        Ok(())
    }
    fn objects(
        &self,
    ) -> Result<Vec<veoveo_gateway_contract::CatalogObjectDescriptor>, ExtensionError> {
        let mut rows = Vec::new();
        for resource in &self.0 {
            rows.push(veoveo_gateway_contract::CatalogObjectDescriptor {
                tenant: None,
                kind: ExtensionName::new("recording_ingest_resource")?,
                id: resource.id.to_string(),
                value: serde_json::to_value(resource)
                    .map_err(|error| ExtensionError::new(error.to_string()))?,
            });
            for producer in &resource.producers {
                rows.push(veoveo_gateway_contract::CatalogObjectDescriptor {
                    tenant: Some(producer.tenant.clone()),
                    kind: ExtensionName::new("recording_producer")?,
                    id: producer.id.to_string(),
                    value: serde_json::to_value(producer)
                        .map_err(|error| ExtensionError::new(error.to_string()))?,
                });
            }
        }
        Ok(rows)
    }
    fn protected_resources(&self) -> Vec<ProtectedResourceDescriptor> {
        self.0
            .iter()
            .map(|resource| ProtectedResourceDescriptor {
                name: resource.id.clone(),
                resource: resource.protected_resource.clone(),
                authorization_server: resource.authorization_server.clone(),
                policy_version: resource.policy_version.clone(),
                required_scopes: resource.required_scopes.clone(),
            })
            .collect()
    }
}

pub fn register_catalog(builder: &mut CatalogRegistryBuilder) -> Result<(), ExtensionError> {
    use std::collections::{BTreeMap, BTreeSet};
    use veoveo_gateway_contract::{
        ActionDescriptor, RuleSelector, SelectorRequirement, ServerRequirement,
    };
    let selectors = BTreeMap::from([
        (RuleSelector::Profiles, SelectorRequirement::Optional),
        (
            RuleSelector::ProtectedResources,
            SelectorRequirement::Required,
        ),
        (RuleSelector::Servers, SelectorRequirement::Forbidden),
        (RuleSelector::Tools, SelectorRequirement::Forbidden),
        (
            RuleSelector::ResourceSchemes,
            SelectorRequirement::Forbidden,
        ),
        (RuleSelector::Prompts, SelectorRequirement::Forbidden),
    ]);
    let mut actions = Vec::new();
    for action in RecordingAction::ALL.iter().copied() {
        let descriptor = if action == RecordingAction::LayerPublish {
            ActionDescriptor {
                access: veoveo_gateway_contract::ActionAccess::Write,
                target_kinds: BTreeSet::from([
                    ExtensionName::new("server")?,
                    ExtensionName::new("resource")?,
                ]),
                selectors: selectors
                    .keys()
                    .map(|selector| (*selector, SelectorRequirement::Optional))
                    .collect(),
                server: Some(ServerRequirement {
                    slug: Some("recording".parse().expect("fixed server")),
                    resources: false,
                }),
            }
        } else {
            ActionDescriptor {
                access: veoveo_gateway_contract::ActionAccess::Write,
                target_kinds: BTreeSet::from([
                    ExtensionName::new("recording_producer")?,
                    ExtensionName::new("recording_stream")?,
                ]),
                selectors: selectors.clone(),
                server: None,
            }
        };
        actions.push((action, descriptor));
    }
    builder.register_actions(actions)?;
    builder.register_section::<RecordingCatalogSection>(ExtensionName::new(
        RECORDING_INGEST_SECTION,
    )?)?;
    builder.register_target::<RecordingTarget>(
        ExtensionName::new(RECORDING_TARGET_GROUP)?,
        vec![
            ExtensionName::new("recording_producer")?,
            ExtensionName::new("recording_stream")?,
        ],
        super::target_audit_resource,
    )?;
    Ok(())
}
