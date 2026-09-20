//! Installation-reviewed runtime templates and their safe authoring projection.
use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{AgentModelId, AgentTemplateId};
use crate::{
    GatewayProfileId, GatewayToolName, ResourceUri, RoleId, ScopeName, SecretReferenceId,
    Sha256Digest, TenantId, WorkContextId, WorkContextMembershipLevel,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ParameterShape {
    Identifier { max_length: u32 },
    Choice { values: Vec<String> },
    Integer { minimum: i64, maximum: i64 },
    Boolean,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParameterChoice {
    pub name: String,
    pub label: String,
    pub shape: ParameterShape,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateParameterBinding {
    pub label: String,
    pub shape: ParameterShape,
    /// Set by installation configuration, never by a definition author.
    pub environment_variable: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateSecretBinding {
    pub reference: SecretReferenceId,
    pub secret: String,
    pub key: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KernelWorkloadTemplate {
    pub namespace: String,
    pub config_map: String,
    /// SHA256 of canonical JSON for the immutable ConfigMap data map.
    pub config_digest: Sha256Digest,
    pub image: String,
    pub database_secret: String,
    pub storage_class: String,
    pub storage_gib: u32,
    pub cpu_millis: u32,
    pub memory_mib: u32,
    pub model_secrets: Vec<TemplateSecretBinding>,
}

/// This configuration is loaded by the gateway and lifecycle manager. It is
/// intentionally absent from the browser schema bundle and public API responses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeTemplate {
    pub id: AgentTemplateId,
    pub name: String,
    pub tenant: TenantId,
    pub work_contexts: Vec<WorkContextId>,
    pub required_deployer_scopes: BTreeSet<ScopeName>,
    pub profile: GatewayProfileId,
    pub scopes: BTreeSet<ScopeName>,
    pub roles: BTreeSet<RoleId>,
    pub membership: WorkContextMembershipLevel,
    pub models: BTreeSet<AgentModelId>,
    pub tools: BTreeSet<GatewayToolName>,
    pub resource_subscriptions: BTreeSet<ResourceUri>,
    pub parameters: BTreeMap<String, TemplateParameterBinding>,
    pub workload: KernelWorkloadTemplate,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateChoice {
    pub roles: Vec<RoleId>,
    pub id: AgentTemplateId,
    pub revision: Sha256Digest,
    pub name: String,
    pub models: Vec<AgentModelId>,
    pub parameters: Vec<ParameterChoice>,
    pub tools: Vec<GatewayToolName>,
    pub resource_subscriptions: Vec<ResourceUri>,
    pub scopes: Vec<ScopeName>,
    pub membership: WorkContextMembershipLevel,
    pub storage_gib: u32,
}
