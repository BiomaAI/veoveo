//! Agent authoring HTTP contracts. Domain invocation continues to use native MCP.
mod ids;
pub use ids::*;
mod instances;
pub use instances::*;
mod templates;
pub use templates::*;

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{GatewayToolName, Sha256Digest, WorkContextId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelReference {
    pub id: AgentModelId,
    pub revision: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Budgets {
    pub max_output_tokens: u32,
    pub max_completion_calls: u32,
    pub max_tool_calls: u32,
    pub deadline_seconds: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum TemplateParameter {
    Text(String),
    Integer(i64),
    Boolean(bool),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Execution {
    Chat,
    Managed {
        template: AgentTemplateId,
        template_revision: Sha256Digest,
        parameters: BTreeMap<String, TemplateParameter>,
        resource_subscriptions: Vec<crate::ResourceUri>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Content {
    pub model: ModelReference,
    pub instructions: String,
    pub tools: Vec<GatewayToolName>,
    pub budgets: Budgets,
    pub execution: Execution,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionStatus {
    Enabled,
    Disabled,
    Archived,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Definition {
    pub id: AgentDefinitionId,
    pub name: String,
    pub description: String,
    pub owner: Uuid,
    pub work_context: WorkContextId,
    pub revision: i64,
    pub status: DefinitionStatus,
    pub disabled: bool,
    pub draft_digest: Sha256Digest,
    pub published_digest: Option<Sha256Digest>,
    pub audience: Vec<WorkContextId>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DefinitionPage {
    pub items: Vec<Definition>,
    pub next: Option<AgentDefinitionId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Draft {
    pub definition: AgentDefinitionId,
    pub revision: i64,
    pub content: Content,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishedRevision {
    pub definition: AgentDefinitionId,
    pub digest: Sha256Digest,
    pub content: Content,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionPage {
    pub items: Vec<PublishedRevision>,
    pub next: Option<Sha256Digest>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateDefinition {
    pub request_id: Uuid,
    pub id: AgentDefinitionId,
    pub name: String,
    pub description: String,
    pub source: DefinitionSource,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DefinitionSource {
    Blank {
        content: Content,
    },
    Duplicate {
        definition: AgentDefinitionId,
        digest: Sha256Digest,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveDraft {
    pub request_id: Uuid,
    pub expected_revision: i64,
    pub content: Content,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateMetadata {
    pub request_id: Uuid,
    pub expected_revision: i64,
    pub change: MetadataChange,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MetadataChange {
    Presentation { name: String, description: String },
    Transfer { owner: Uuid },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishDefinition {
    pub request_id: Uuid,
    pub expected_revision: i64,
    pub digest: Sha256Digest,
    pub audience: Vec<WorkContextId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionRequest {
    pub request_id: Uuid,
    pub expected_revision: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidateDefinition {
    pub expected_revision: i64,
    pub audience: Vec<WorkContextId>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FindingCode {
    InvalidContent,
    ModelUnavailable,
    ModelChanged,
    CapabilityUnavailable,
    CapacityExceeded,
    TemplateUnavailable,
    AudienceForbidden,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Finding {
    pub code: FindingCode,
    pub field: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Validation {
    pub revision: i64,
    pub digest: Sha256Digest,
    pub findings: Vec<Finding>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelChoice {
    pub reference: ModelReference,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub limits: Budgets,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityChoice {
    pub name: GatewayToolName,
    pub title: String,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthoringPermissions {
    pub read_content: bool,
    pub create: bool,
    pub edit: bool,
    pub publish: bool,
    pub control: bool,
    pub archive: bool,
    pub transfer: bool,
    pub deploy: bool,
    pub manage_context: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Authoring {
    pub work_context: WorkContextId,
    pub permissions: AuthoringPermissions,
    pub models: Vec<ModelChoice>,
    pub definition_limit: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogWake {
    pub sequence: i64,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct AgentManagementSchema {
    template: TemplateChoice,
    instance: ManagedInstance,
    instances: InstancePage,
    provision: ProvisionInstance,
    update_instance: UpdateInstance,
    lifecycle: LifecycleOperation,
    definition: Definition,
    page: DefinitionPage,
    draft: Draft,
    revisions: RevisionPage,
    create: CreateDefinition,
    save: SaveDraft,
    metadata: UpdateMetadata,
    publish: PublishDefinition,
    revision: RevisionRequest,
    validate: ValidateDefinition,
    validation: Validation,
    authoring: Authoring,
    capability: CapabilityChoice,
    wake: CatalogWake,
}

pub fn schema_bundle() -> schemars::Schema {
    schemars::schema_for!(AgentManagementSchema)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authoring_never_accepts_provider_credentials_or_implicit_authority() {
        let content = serde_json::json!({
            "model":{"id":"approved","revision":format!("sha256:{}", "a".repeat(64))},
            "instructions":"A bounded assistant", "tools":[], "execution":{"kind":"chat"},
            "budgets":{"maxOutputTokens":128,"maxCompletionCalls":4,"maxToolCalls":8,"deadlineSeconds":120}
        });
        assert!(serde_json::from_value::<Content>(content.clone()).is_ok());
        for field in ["apiKey", "baseUrl", "owner", "tenant"] {
            let mut forged = content.clone();
            forged[field] = serde_json::json!("forbidden");
            assert!(serde_json::from_value::<Content>(forged).is_err());
        }
        assert!(AgentDefinitionId::new("../escape").is_err());
        assert!(AgentDefinitionId::new("a".repeat(129)).is_err());
    }
}
