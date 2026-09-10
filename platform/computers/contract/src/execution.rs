use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Governed occurrence reference. Bytes remain behind Artifact read authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionOutput {
    pub artifact_id: Uuid,
    #[schemars(range(max = 67108864))]
    pub byte_count: u32,
}

/// A known foreground result. Nonzero exit is a completed command with a tool
/// error; it does not imply transport failure or termination of detached children.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionResult {
    pub computer_id: Uuid,
    pub execution_id: Uuid,
    pub exit_code: u8,
    pub stdout: ExecutionOutput,
    pub stderr: ExecutionOutput,
}
