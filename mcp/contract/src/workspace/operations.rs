//! Closed browser projections over native MCP Tasks and tool continuations.
use super::{AgentId, ChatId, OperationId, RunId};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capability {
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    /// A domain-owned JSON Schema, not a Workspace-controlled record shape.
    pub input_schema: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartOperation {
    pub id: OperationId,
    pub tool: String,
    pub arguments: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperationPhase {
    Dispatching,
    InputRequired,
    Task,
    Completed,
    Failed,
    Unconfirmed,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationSummary {
    pub id: OperationId,
    pub chat_id: ChatId,
    pub run_id: Option<RunId>,
    pub agent: Option<OperationAgent>,
    pub tool: String,
    pub phase: OperationPhase,
    pub revision: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationAgent {
    pub id: AgentId,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationPage {
    pub items: Vec<OperationSummary>,
    pub next: Option<OperationId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Working,
    InputRequired,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskView {
    pub id: String,
    pub state: TaskState,
    pub message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub ttl_ms: Option<u64>,
    pub poll_interval_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    Form,
    Link,
    Unsupported,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationInput {
    /// Native request key plus content digest fences a stale rendered form.
    pub id: String,
    pub digest: String,
    pub kind: InputKind,
    pub message: String,
    pub schema: Option<serde_json::Value>,
    pub url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputDecision {
    Accept,
    Decline,
    Cancel,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputAnswer {
    pub id: String,
    pub digest: String,
    pub decision: InputDecision,
    /// A domain's elicitation form can have different primitive fields.
    pub content: Option<BTreeMap<String, serde_json::Value>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnswerOperation {
    pub revision: i64,
    pub answers: Vec<InputAnswer>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationResource {
    pub uri: String,
    pub name: String,
    pub mime_type: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ResultImageMime {
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/webp")]
    Webp,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationImage {
    pub mime_type: ResultImageMime,
    /// Validated standard base64; the complete result admits at most eight
    /// images and 1 MiB of encoded image data.
    pub data: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationResult {
    pub is_error: bool,
    pub text: Vec<String>,
    pub resources: Vec<OperationResource>,
    pub images: Vec<OperationImage>,
    pub omitted_images: u32,
    pub structured: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationView {
    pub operation: OperationSummary,
    pub progress: Option<OperationProgress>,
    pub task: Option<TaskView>,
    pub inputs: Vec<OperationInput>,
    pub result: Option<OperationResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationProgress {
    pub completed: f64,
    pub total: Option<f64>,
    pub message: Option<String>,
}
