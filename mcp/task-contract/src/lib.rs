//! Storage-independent identities used by the final MCP task extension.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// Stable pin that prevents pruning a task result while another owner needs it.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct TaskRetentionPin(String);

impl TaskRetentionPin {
    pub fn new(value: impl Into<String>) -> Result<Self, TaskRetentionPinError> {
        let value = value.into();
        if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(TaskRetentionPinError);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TaskRetentionPin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for TaskRetentionPin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("task retention pin is empty, too long, or contains a control character")]
pub struct TaskRetentionPinError;

/// UUIDv7 identity carried on the MCP task wire.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProtocolTaskId(Uuid);

impl ProtocolTaskId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }

    #[cfg(feature = "runtime")]
    pub const fn from_task_id(task_id: veoveo_platform_store::TaskId) -> Self {
        Self(task_id.as_uuid())
    }

    #[cfg(feature = "runtime")]
    pub const fn task_id(self) -> veoveo_platform_store::TaskId {
        veoveo_platform_store::TaskId::from_uuid(self.0)
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

#[cfg(feature = "runtime")]
impl From<veoveo_platform_store::TaskId> for ProtocolTaskId {
    fn from(value: veoveo_platform_store::TaskId) -> Self {
        Self::from_task_id(value)
    }
}

impl FromStr for ProtocolTaskId {
    type Err = ProtocolTaskIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let uuid = Uuid::parse_str(value)?;
        if uuid.get_version_num() != 7 {
            return Err(ProtocolTaskIdError::UnsupportedVersion);
        }
        Ok(Self(uuid))
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

#[cfg(test)]
mod tests {
    use super::{ProtocolTaskId, TaskRetentionPin};

    #[test]
    fn wire_task_ids_require_uuid_v7() {
        let id = ProtocolTaskId::new();
        assert_eq!(id.to_string().parse::<ProtocolTaskId>().unwrap(), id);
        assert!(
            "00000000-0000-4000-8000-000000000000"
                .parse::<ProtocolTaskId>()
                .is_err()
        );
    }

    #[test]
    fn retention_pins_are_bounded_tokens() {
        assert!(TaskRetentionPin::new("episode:123").is_ok());
        assert!(TaskRetentionPin::new("").is_err());
        assert!(TaskRetentionPin::new("x".repeat(257)).is_err());
    }
}
