use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One explicit command. Arguments and environment can contain secrets and have
/// no diagnostic formatting surface. Directory is relative to the retained home.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecuteInput {
    pub computer_id: Uuid,
    pub grant_id: Uuid,
    pub request_id: Uuid,
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

/// Canonical address for one completed command; it is not an access credential.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ExecutionResultUri(Uuid);
impl ExecutionResultUri {
    pub fn new(execution_id: Uuid) -> Result<Self, &'static str> {
        if execution_id.get_version_num() != 7 {
            return Err("invalid Computer execution result URI");
        }
        Ok(Self(execution_id))
    }
    pub fn execution_id(self) -> Uuid {
        self.0
    }
}
impl From<ExecutionResultUri> for String {
    fn from(uri: ExecutionResultUri) -> Self {
        format!("computer://executions/{}", uri.0)
    }
}
impl TryFrom<String> for ExecutionResultUri {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let invalid = "invalid Computer execution result URI";
        let id = value
            .strip_prefix("computer://executions/")
            .ok_or(invalid)?;
        let id = Uuid::parse_str(id).map_err(|_| invalid)?;
        let uri = Self::new(id)?;
        if String::from(uri) != value {
            return Err(invalid);
        }
        Ok(uri)
    }
}

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
    #[serde(rename = "result_uri")]
    #[schemars(
        with = "String",
        regex(
            pattern = "^computer://executions/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
        )
    )]
    pub result_uri: ExecutionResultUri,
    pub computer_id: Uuid,
    pub execution_id: Uuid,
    pub exit_code: u8,
    pub stdout: ExecutionOutput,
    pub stderr: ExecutionOutput,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn execution_addresses_reject_alternate_paths_encodings_and_non_command_ids() {
        let id = Uuid::now_v7();
        let canonical = String::from(ExecutionResultUri::new(id).unwrap());
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
            format!("computer://executions/{}", id.simple()),
            format!("computer://executions/{}", Uuid::nil()),
            format!("computer://computers/{id}"),
        ] {
            assert!(ExecutionResultUri::try_from(invalid).is_err());
        }
    }
}
