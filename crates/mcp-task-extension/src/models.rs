use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use veoveo_platform_store::TaskId;
use veoveo_task_runtime::TaskRetentionPin;

pub const PROTOCOL_VERSION: &str = "2026-06-30";
pub const EXTENSION_ID: &str = "io.modelcontextprotocol/tasks";
pub const DISCOVER_METHOD: &str = "server/discover";
pub const GET_TASK_METHOD: &str = "tasks/get";
pub const UPDATE_TASK_METHOD: &str = "tasks/update";
pub const CANCEL_TASK_METHOD: &str = "tasks/cancel";
pub const LISTEN_METHOD: &str = "subscriptions/listen";
pub const TASK_NOTIFICATION_METHOD: &str = "notifications/tasks";
pub const SUBSCRIPTION_ACKNOWLEDGED_METHOD: &str = "notifications/subscriptions/acknowledged";
pub const CLIENT_CAPABILITIES_META_KEY: &str = "io.modelcontextprotocol/clientCapabilities";
pub const PROTOCOL_VERSION_META_KEY: &str = "io.modelcontextprotocol/protocolVersion";
pub const SUBSCRIPTION_ID_META_KEY: &str = "io.modelcontextprotocol/subscriptionId";
pub const TASK_RETENTION_PIN_META_KEY: &str = "ai.bioma.veoveo/taskRetentionPin";
pub const MISSING_REQUIRED_CLIENT_CAPABILITY: i32 = -32_003;
pub const HEADER_MCP_METHOD: &str = "mcp-method";
pub const HEADER_MCP_NAME: &str = "mcp-name";
pub const HEADER_MCP_PROTOCOL_VERSION: &str = "mcp-protocol-version";

pub type JsonObject = BTreeMap<String, Value>;
pub type InputRequests = BTreeMap<String, EmbeddedRequest>;
pub type InputResponses = BTreeMap<String, JsonObject>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskExtensionCapability {}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClientCapabilities {
    #[serde(default)]
    pub extensions: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub additional: JsonObject,
}

impl ClientCapabilities {
    pub fn declares_tasks(&self) -> bool {
        self.extensions.get(EXTENSION_ID).is_some_and(|value| {
            serde_json::from_value::<TaskExtensionCapability>(value.clone()).is_ok()
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RequestMeta {
    #[serde(rename = "io.modelcontextprotocol/protocolVersion")]
    pub protocol_version: String,
    #[serde(
        rename = "io.modelcontextprotocol/clientCapabilities",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub client_capabilities: Option<ClientCapabilities>,
    #[serde(
        rename = "ai.bioma.veoveo/taskRetentionPin",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub task_retention_pin: Option<TaskRetentionPin>,
    #[serde(flatten)]
    pub additional: JsonObject,
}

impl RequestMeta {
    pub fn new() -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            client_capabilities: None,
            task_retention_pin: None,
            additional: JsonObject::new(),
        }
    }

    pub fn with_task_capability(mut self) -> Self {
        self.client_capabilities = Some(ClientCapabilities {
            extensions: BTreeMap::from([(
                EXTENSION_ID.to_owned(),
                Value::Object(Default::default()),
            )]),
            additional: JsonObject::new(),
        });
        self
    }

    pub fn with_retention_pin(mut self, pin: TaskRetentionPin) -> Self {
        self.task_retention_pin = Some(pin);
        self
    }

    pub fn declares_tasks(&self) -> bool {
        self.client_capabilities
            .as_ref()
            .is_some_and(ClientCapabilities::declares_tasks)
    }
}

impl Default for RequestMeta {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProtocolTaskId(TaskId);

impl ProtocolTaskId {
    pub fn new() -> Self {
        Self(TaskId::new())
    }

    pub const fn from_task_id(task_id: TaskId) -> Self {
        Self(task_id)
    }

    pub const fn task_id(self) -> TaskId {
        self.0
    }
}

impl DetailedTask {
    pub const fn metadata(&self) -> &TaskMetadata {
        match self {
            Self::Working { metadata }
            | Self::InputRequired { metadata, .. }
            | Self::Completed { metadata, .. }
            | Self::Failed { metadata, .. }
            | Self::Cancelled { metadata } => metadata,
        }
    }

    pub const fn status(&self) -> TaskStatus {
        match self {
            Self::Working { .. } => TaskStatus::Working,
            Self::InputRequired { .. } => TaskStatus::InputRequired,
            Self::Completed { .. } => TaskStatus::Completed,
            Self::Failed { .. } => TaskStatus::Failed,
            Self::Cancelled { .. } => TaskStatus::Cancelled,
        }
    }
}

impl Default for ProtocolTaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ProtocolTaskId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<TaskId> for ProtocolTaskId {
    fn from(value: TaskId) -> Self {
        Self::from_task_id(value)
    }
}

impl FromStr for ProtocolTaskId {
    type Err = ProtocolTaskIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let uuid = uuid::Uuid::parse_str(value)?;
        if uuid.get_version_num() != 7 {
            return Err(ProtocolTaskIdError::UnsupportedVersion);
        }
        Ok(Self(TaskId::from_uuid(uuid)))
    }
}

impl Serialize for ProtocolTaskId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ProtocolTaskId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolTaskIdError {
    #[error("invalid task UUID: {0}")]
    InvalidUuid(#[from] uuid::Error),
    #[error("task id must be a UUIDv7")]
    UnsupportedVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Working,
    InputRequired,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub task_id: ProtocolTaskId,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_interval_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMetadata {
    pub task_id: ProtocolTaskId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_interval_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DetailedTask {
    Working {
        #[serde(flatten)]
        metadata: TaskMetadata,
    },
    InputRequired {
        #[serde(flatten)]
        metadata: TaskMetadata,
        #[serde(rename = "inputRequests")]
        input_requests: InputRequests,
    },
    Completed {
        #[serde(flatten)]
        metadata: TaskMetadata,
        result: JsonObject,
    },
    Failed {
        #[serde(flatten)]
        metadata: TaskMetadata,
        error: JsonRpcErrorData,
    },
    Cancelled {
        #[serde(flatten)]
        metadata: TaskMetadata,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddedRequest {
    pub method: String,
    #[serde(default)]
    pub params: JsonObject,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcErrorData {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TaskResultType;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CompleteResultType;

macro_rules! literal_result_type {
    ($type:ty, $literal:literal) => {
        impl Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str($literal)
            }
        }

        impl<'de> Deserialize<'de> for $type {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                if value == $literal {
                    Ok(Self)
                } else {
                    Err(serde::de::Error::custom(concat!(
                        "resultType must be `",
                        $literal,
                        "`"
                    )))
                }
            }
        }
    };
}

literal_result_type!(TaskResultType, "task");
literal_result_type!(CompleteResultType, "complete");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskResult {
    result_type: TaskResultType,
    #[serde(flatten)]
    pub task: Task,
}

impl CreateTaskResult {
    pub fn new(task: Task) -> Self {
        Self {
            result_type: TaskResultType,
            task,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTaskResult {
    result_type: CompleteResultType,
    #[serde(flatten)]
    pub task: DetailedTask,
}

impl GetTaskResult {
    pub fn new(task: DetailedTask) -> Self {
        Self {
            result_type: CompleteResultType,
            task,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetTaskParams {
    #[serde(rename = "_meta")]
    pub meta: RequestMeta,
    pub task_id: ProtocolTaskId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateTaskParams {
    #[serde(rename = "_meta")]
    pub meta: RequestMeta,
    pub task_id: ProtocolTaskId,
    pub input_responses: InputResponses,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CancelTaskParams {
    #[serde(rename = "_meta")]
    pub meta: RequestMeta,
    pub task_id: ProtocolTaskId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ToolCallParams {
    #[serde(rename = "_meta")]
    pub meta: RequestMeta,
    pub name: String,
    #[serde(default)]
    pub arguments: JsonObject,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AcknowledgeTaskResult {
    #[serde(rename = "resultType")]
    result_type: CompleteResultType,
}

impl AcknowledgeTaskResult {
    pub const fn complete() -> Self {
        Self {
            result_type: CompleteResultType,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoverParams {
    #[serde(rename = "_meta")]
    pub meta: RequestMeta,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverResult {
    #[serde(rename = "resultType")]
    result_type: CompleteResultType,
    pub supported_versions: Vec<String>,
    pub capabilities: JsonObject,
    pub server_info: Implementation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

impl DiscoverResult {
    pub fn new(
        capabilities: JsonObject,
        server_info: Implementation,
        instructions: Option<String>,
    ) -> Self {
        Self {
            result_type: CompleteResultType,
            supported_versions: vec![PROTOCOL_VERSION.to_owned()],
            capabilities,
            server_info,
            instructions,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Implementation {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct NotificationSelection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_ids: Option<Vec<ProtocolTaskId>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListenParams {
    #[serde(rename = "_meta")]
    pub meta: RequestMeta,
    pub notifications: NotificationSelection,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_and_discriminators_match_final_sep() {
        assert_eq!(PROTOCOL_VERSION, "2026-06-30");
        assert_eq!(EXTENSION_ID, "io.modelcontextprotocol/tasks");
        assert_eq!(UPDATE_TASK_METHOD, "tasks/update");
        assert_eq!(
            TASK_RETENTION_PIN_META_KEY,
            "ai.bioma.veoveo/taskRetentionPin"
        );
        assert_eq!(
            serde_json::to_value(AcknowledgeTaskResult::complete()).unwrap(),
            serde_json::json!({"resultType": "complete"})
        );
    }

    #[test]
    fn request_ids_reject_non_v7_uuids() {
        let value = serde_json::json!({
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
                "io.modelcontextprotocol/clientCapabilities": {
                    "extensions": { EXTENSION_ID: {} }
                }
            },
            "taskId": uuid::Uuid::new_v4().to_string()
        });
        assert!(serde_json::from_value::<GetTaskParams>(value).is_err());
    }

    #[test]
    fn retention_pin_meta_is_typed_and_validated() {
        let meta: RequestMeta = serde_json::from_value(serde_json::json!({
            "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
            "ai.bioma.veoveo/taskRetentionPin": "agent-episode:019"
        }))
        .unwrap();
        assert_eq!(
            meta.task_retention_pin.as_ref().map(|pin| pin.as_str()),
            Some("agent-episode:019")
        );
        assert!(
            serde_json::from_value::<RequestMeta>(serde_json::json!({
                "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
                "ai.bioma.veoveo/taskRetentionPin": "bad\nvalue"
            }))
            .is_err()
        );
    }
}
