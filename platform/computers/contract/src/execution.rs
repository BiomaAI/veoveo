use crate::ExecutionResultUri;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use uuid::Uuid;

/// One explicit command. Arguments and environment can contain secrets and have
/// no diagnostic formatting surface. Directory is relative to the retained home.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecuteInput {
    pub computer_id: crate::ComputerId,
    pub grant_id: crate::AutomationGrantId,
    pub request_id: crate::RequestId,
    #[schemars(length(min = 1, max = 1024))]
    pub arguments: Vec<String>,
    #[schemars(length(min = 1, max = 1024))]
    pub directory: String,
    pub environment: std::collections::BTreeMap<String, String>,
    /// RFC 4648 standard padded base64; at most 1 MiB decoded.
    #[schemars(length(max = 1398104))]
    pub stdin: String,
    pub limits: crate::AutomationExecutionLimits,
}

/// Governed occurrence reference. Bytes remain behind Artifact read authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionOutput {
    pub artifact_id: crate::ArtifactId,
    #[schemars(range(max = 67108864))]
    pub byte_count: u32,
}

/// A known foreground result. Nonzero exit is a completed command with a tool
/// error; it does not imply transport failure or termination of detached children.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionResult {
    #[serde(rename = "result_uri")]
    #[schemars(
        with = "String",
        regex(
            pattern = "^computer://executions/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
        )
    )]
    result_uri: ExecutionResultUri,
    computer_id: crate::ComputerId,
    execution_id: crate::ExecutionId,
    #[schemars(schema_with = "completed_exit_schema")]
    exit_code: u8,
    stdout: ExecutionOutput,
    stderr: ExecutionOutput,
}
impl ExecutionResult {
    pub fn new(
        computer_id: crate::ComputerId,
        execution_id: crate::ExecutionId,
        exit_code: u8,
        stdout: ExecutionOutput,
        stderr: ExecutionOutput,
    ) -> Result<Self, crate::ComputerResultError> {
        if exit_code == 124
            || stdout.artifact_id == stderr.artifact_id
            || u64::from(stdout.byte_count) + u64::from(stderr.byte_count)
                > crate::MAX_TRANSFER_BYTES
        {
            return Err(crate::ComputerResultError);
        }
        Ok(Self {
            result_uri: ExecutionResultUri::new(execution_id),
            computer_id,
            execution_id,
            exit_code,
            stdout,
            stderr,
        })
    }
    pub fn result_uri(&self) -> ExecutionResultUri {
        self.result_uri
    }
    pub fn computer_id(&self) -> crate::ComputerId {
        self.computer_id
    }
    pub fn execution_id(&self) -> crate::ExecutionId {
        self.execution_id
    }
    pub fn exit_code(&self) -> u8 {
        self.exit_code
    }
    pub fn stdout(&self) -> ExecutionOutput {
        self.stdout
    }
    pub fn stderr(&self) -> ExecutionOutput {
        self.stderr
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExecutionResultWire {
    #[serde(rename = "result_uri")]
    #[schemars(
        with = "String",
        regex(
            pattern = "^computer://executions/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
        )
    )]
    result_uri: ExecutionResultUri,
    computer_id: crate::ComputerId,
    execution_id: crate::ExecutionId,
    #[schemars(schema_with = "completed_exit_schema")]
    exit_code: u8,
    stdout: ExecutionOutput,
    stderr: ExecutionOutput,
}

fn completed_exit_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"anyOf": [{"type": "integer", "minimum": 0, "maximum": 123}, {"type": "integer", "minimum": 125, "maximum": 255}]})
}

impl<'de> Deserialize<'de> for ExecutionResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = ExecutionResultWire::deserialize(deserializer)?;

        if wire.result_uri.execution_id() != wire.execution_id {
            return Err(serde::de::Error::custom(crate::ComputerResultError));
        }
        Self::new(
            wire.computer_id,
            wire.execution_id,
            wire.exit_code,
            wire.stdout,
            wire.stderr,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn execution_addresses_reject_alternate_paths_encodings_and_non_command_ids() {
        let id = crate::ExecutionId::new();
        let canonical = String::from(ExecutionResultUri::new(id));
        assert_eq!(
            ExecutionResultUri::try_from(canonical.clone())
                .unwrap()
                .execution_id(),
            id
        );
        for invalid in [
            canonical.to_uppercase(),
            format!("{canonical}/"),
            format!("{canonical}?other=1"),
            format!("computer://executions/{}", id.as_uuid().simple()),
            format!("computer://executions/{}", Uuid::nil()),
            format!("computer://computers/{id}"),
        ] {
            assert!(ExecutionResultUri::try_from(invalid).is_err());
        }
    }
}
