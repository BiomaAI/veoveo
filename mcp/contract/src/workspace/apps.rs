//! App bridge envelopes. Native result fields retain the pinned MCP SDK types.
use super::{OperationId, OperationSummary};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartAppOperation {
    pub id: OperationId,
    pub app_uri: String,
    pub tool: String,
    pub arguments: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub request_state: Option<String>,
    #[serde(default)]
    pub input_responses: Option<rmcp::model::InputResponses>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppOrigin {
    pub app_uri: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppOperationView {
    pub operation: OperationSummary,
    /// Native MCP 2026-07-28 result, validated by the pinned App protocol adapter.
    #[schemars(with = "Option<serde_json::Value>")]
    pub native: Option<AppToolResult>,
}

/// The SDK's dispatch enum is not serializable. Its three typed wire results
/// keep their native discriminators across the browser-edge envelope.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AppToolResult {
    Task(rmcp::model::CreateTaskResult),
    InputRequired(rmcp::model::InputRequiredResult),
    Complete(rmcp::model::CallToolResult),
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppTaskRequest {
    pub app_uri: String,
    pub task_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateAppTask {
    pub app_uri: String,
    pub task_id: String,
    pub input_responses: rmcp::model::InputResponses,
}
