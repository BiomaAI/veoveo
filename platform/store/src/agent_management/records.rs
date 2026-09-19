use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types as surrealdb_types;
use surrealdb::types::{RecordId, SurrealValue, Value};
use uuid::Uuid;

use crate::{PrincipalId, TenantId, WorkContextId, WorkContextMembershipLevel};

/// Authenticated server decision, never accepted from a public request body.
/// The caller evaluates the exact management action before constructing this value.
#[derive(Clone, SurrealValue)]
pub struct AgentCatalogAuthority {
    pub tenant: RecordId,
    pub work_context: RecordId,
    pub principal: RecordId,
    pub context_digest: String,
    pub membership: WorkContextMembershipLevel,
    pub manage_context: bool,
    pub definition_limit: u32,
}

impl AgentCatalogAuthority {
    pub fn new(
        tenant: TenantId,
        context: WorkContextId,
        principal: PrincipalId,
        context_digest: String,
        membership: WorkContextMembershipLevel,
        manage_context: bool,
        definition_limit: u32,
    ) -> Self {
        Self {
            tenant: tenant.record_id(),
            work_context: context.record_id(),
            principal: principal.record_id(),
            context_digest,
            membership,
            manage_context,
            definition_limit,
        }
    }
}

/// Current publication authority for an additional context, resolved by the server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct AgentPublicationContext {
    pub work_context: RecordId,
    pub context_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(deny_unknown_fields)]
pub struct AgentModelReference {
    pub id: String,
    /// Public configuration digest. Credential rotation does not change this digest.
    pub revision: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(deny_unknown_fields)]
pub struct AgentBudgets {
    pub max_output_tokens: u32,
    pub max_completion_calls: u32,
    pub max_tool_calls: u32,
    pub deadline_seconds: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(untagged)]
#[surreal(untagged)]
pub enum AgentTemplateParameter {
    Text(String),
    Integer(i64),
    Boolean(bool),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[surreal(tag = "kind")]
pub enum AgentExecution {
    #[surreal(rename = "chat")]
    Chat,
    #[surreal(rename = "managed")]
    Managed {
        template: String,
        template_revision: String,
        parameters: BTreeMap<String, AgentTemplateParameter>,
        resource_subscriptions: Vec<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
#[surreal(untagged)]
pub enum AgentExecutionKind {
    #[surreal(value = "chat")]
    Chat,
    #[surreal(value = "managed")]
    Managed,
}

/// Executable configuration contains references, never provider destinations or secrets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(deny_unknown_fields)]
pub struct AgentContent {
    pub model: AgentModelReference,
    pub instructions: String,
    pub tools: Vec<String>,
    pub budgets: AgentBudgets,
    pub execution: AgentExecution,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
#[surreal(untagged)]
pub enum AgentDefinitionStatus {
    #[surreal(value = "enabled")]
    Enabled,
    #[surreal(value = "disabled")]
    Disabled,
    #[surreal(value = "archived")]
    Archived,
}

/// Private authoring record. Never serialize directly into a discovery response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct AgentDefinition {
    pub id: RecordId,
    pub tenant: RecordId,
    pub work_context: RecordId,
    pub owner: RecordId,
    pub key: String,
    pub name: String,
    pub description: String,
    pub revision: i64,
    pub status: AgentDefinitionStatus,
    pub disabled: bool,
    pub audience: Vec<RecordId>,
    pub draft: AgentContent,
    pub draft_digest: String,
    pub published: Option<RecordId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct AgentRevision {
    pub id: RecordId,
    pub definition: RecordId,
    pub digest: String,
    pub content: AgentContent,
    pub created_by: RecordId,
    pub created_at: DateTime<Utc>,
}

/// Public catalog metadata. No instruction, template parameter or credential field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct AgentCatalogEntry {
    pub key: String,
    pub name: String,
    pub description: String,
    pub revision: i64,
    pub digest: String,
    pub model: AgentModelReference,
    pub tools: Vec<String>,
    pub execution_kind: AgentExecutionKind,
}

#[derive(Clone, Debug, Serialize, SurrealValue)]
#[surreal(tag = "kind")]
pub enum AgentDefinitionMutation {
    #[surreal(rename = "create")]
    Create {
        name: String,
        description: String,
        content: AgentContent,
    },
    #[surreal(rename = "draft")]
    Draft { content: AgentContent },
    #[surreal(rename = "publish")]
    Publish {
        digest: String,
        audience: Vec<AgentPublicationContext>,
    },
    #[surreal(rename = "metadata")]
    Metadata { name: String, description: String },
    #[surreal(rename = "status")]
    Status { status: AgentDefinitionStatus },
    #[surreal(rename = "transfer")]
    Transfer { owner: RecordId },
}

#[derive(Clone, SurrealValue)]
pub(super) struct MutationCommand {
    pub definition: RecordId,
    pub receipt: RecordId,
    pub request_id: Uuid,
    pub fingerprint: String,
    pub key: String,
    pub expected_revision: Option<i64>,
    pub content_digest: Option<String>,
    pub mutation: AgentDefinitionMutation,
}
