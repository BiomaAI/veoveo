//! Regular-file handoff. Artifact references and paths confer no authority.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_TRANSFER_BYTES: u64 = 64 * 1024 * 1024;

/// Canonical relative Unix path. Omit diagnostic formatting for private filenames.
#[derive(Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct RetainedFilePath(String);

impl JsonSchema for RetainedFilePath {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "RetainedFilePath".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type":"string","minLength":1,"maxLength":1024,
            "description":"Canonical relative path inside the retained home; at most 1024 UTF-8 bytes, without traversal or control characters."})
    }
}

impl TryFrom<String> for RetainedFilePath {
    type Error = &'static str;
    fn try_from(path: String) -> Result<Self, Self::Error> {
        if path.is_empty()
            || path.len() > 1024
            || path.chars().any(char::is_control)
            || path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err("use a relative path inside the retained home without traversal");
        }
        Ok(Self(path))
    }
}
impl RetainedFilePath {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileTransferDirection {
    Import,
    Export,
}

/// Archives remain opaque regular files. Imports always create a new destination.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum FileTransfer {
    Import {
        artifact_id: Uuid,
        path: RetainedFilePath,
    },
    Export {
        path: RetainedFilePath,
        #[schemars(length(min = 1, max = 255))]
        filename: String,
        #[schemars(length(min = 1, max = 255))]
        media_type: String,
    },
}
impl FileTransfer {
    pub fn direction(&self) -> FileTransferDirection {
        match self {
            Self::Import { .. } => FileTransferDirection::Import,
            Self::Export { .. } => FileTransferDirection::Export,
        }
    }
    pub fn path(&self) -> &RetainedFilePath {
        match self {
            Self::Import { path, .. } | Self::Export { path, .. } => path,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileTransferLimits {
    #[schemars(range(min = 1, max = 300))]
    pub maximum_seconds: u32,
    #[schemars(range(min = 1, max = 67108864))]
    pub maximum_bytes: u64,
    /// Interrupting an active transfer may stop all processes on the Computer.
    pub on_interruption: crate::AutomationInterruption,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferFileInput {
    pub computer_id: Uuid,
    pub request_id: Uuid,
    /// A direct owner omits this field. Delegation requires a current named grant.
    pub grant_id: Option<Uuid>,
    pub transfer: FileTransfer,
    pub limits: FileTransferLimits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileTransferStage {
    Queued,
    Dispatched,
    Containing,
    RecoveryRequired,
    Completed,
    Failed,
    Cancelled,
}

/// Canonical address for one completed file transfer; it is not an access credential.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FileTransferResultUri(Uuid);
impl FileTransferResultUri {
    pub fn new(transfer_id: Uuid) -> Result<Self, &'static str> {
        if transfer_id.get_version_num() != 7 {
            return Err("invalid Computer file transfer result URI");
        }
        Ok(Self(transfer_id))
    }
    pub fn transfer_id(self) -> Uuid {
        self.0
    }
}
impl From<FileTransferResultUri> for String {
    fn from(uri: FileTransferResultUri) -> Self {
        format!("computer://transfers/{}", uri.0)
    }
}
impl TryFrom<String> for FileTransferResultUri {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let invalid = "invalid Computer file transfer result URI";
        let id = value.strip_prefix("computer://transfers/").ok_or(invalid)?;
        let id = Uuid::parse_str(id).map_err(|_| invalid)?;
        let uri = Self::new(id)?;
        if String::from(uri) != value {
            return Err(invalid);
        }
        Ok(uri)
    }
}

/// Metadata only. Artifact reads remain governed by the Artifact service.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileTransferResult {
    #[serde(rename = "result_uri")]
    #[schemars(
        with = "String",
        regex(
            pattern = "^computer://transfers/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
        )
    )]
    pub result_uri: FileTransferResultUri,
    pub computer_id: Uuid,
    pub transfer_id: Uuid,
    pub direction: FileTransferDirection,
    pub artifact_id: Uuid,
    #[schemars(range(max = 67108864))]
    pub bytes: u64,
    #[schemars(regex(pattern = "^[0-9a-f]{64}$"))]
    pub sha256: String,
}

/// Native Console projection of the same durable Task used by MCP callers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileTransferView {
    pub task_id: Uuid,
    pub computer_id: Uuid,
    pub direction: FileTransferDirection,
    pub stage: FileTransferStage,
    #[schemars(length(max = 4096))]
    pub message: Option<String>,
    pub cancellation_requested_at: Option<chrono::DateTime<chrono::Utc>>,
    pub can_cancel: bool,
    pub result: Option<FileTransferResult>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CancelFileTransferBody {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_uri_accepts_only_the_exact_uuidv7_address() {
        let id = Uuid::now_v7();
        let uri = format!("computer://transfers/{id}");
        assert_eq!(
            FileTransferResultUri::try_from(uri.clone())
                .unwrap()
                .transfer_id(),
            id
        );
        for invalid in [
            uri.to_uppercase(),
            format!("{uri}/"),
            format!("{uri}?token=secret"),
            format!("computer://transfers/{}", id.simple()),
            format!("computer://transfers/{}", Uuid::nil()),
            "computer://transfers/00000000-0000-4000-8000-000000000001".into(),
        ] {
            assert!(FileTransferResultUri::try_from(invalid).is_err());
        }
        assert_eq!(
            serde_json::to_value(FileTransferResultUri::new(id).unwrap()).unwrap(),
            uri
        );
    }

    #[test]
    fn paths_reject_traversal_and_preserve_unicode_without_normalization() {
        let schema = serde_json::to_value(crate::schema_bundle()).unwrap();
        assert_eq!(schema["$defs"]["RetainedFilePath"]["minLength"], 1);
        assert_eq!(schema["$defs"]["RetainedFilePath"]["maxLength"], 1024);
        for value in [
            "",
            "/etc/passwd",
            "../file",
            "a/../b",
            "a/./b",
            "a//b",
            "a/",
            "a\0b",
            "a\nb",
        ] {
            assert!(serde_json::from_value::<RetainedFilePath>(value.into()).is_err());
        }
        assert!(RetainedFilePath::try_from("雪".repeat(342)).is_err());
        let path: RetainedFilePath =
            serde_json::from_value("project/雪 archive.tar".into()).unwrap();
        assert_eq!(path.as_str(), "project/雪 archive.tar");
        assert_eq!(
            serde_json::to_value(path).unwrap(),
            "project/雪 archive.tar"
        );
    }

    #[test]
    fn transfer_wire_rejects_extraction_overwrite_and_hidden_authority() {
        let request = serde_json::json!({
            "computerId":Uuid::nil(), "requestId":Uuid::nil(), "grantId":null,
            "transfer":{"kind":"import", "artifactId":Uuid::nil(), "path":"data.bin"},
            "limits":{"maximumSeconds":30, "maximumBytes":1024, "onInterruption":"stop_computer"}
        });
        assert!(serde_json::from_value::<TransferFileInput>(request.clone()).is_ok());
        for extra in ["overwrite", "extract", "owner", "provider"] {
            let mut invalid = request.clone();
            invalid["transfer"][extra] = true.into();
            assert!(serde_json::from_value::<TransferFileInput>(invalid).is_err());
        }
        let mut invalid = request;
        invalid["limits"]
            .as_object_mut()
            .unwrap()
            .remove("onInterruption");
        assert!(serde_json::from_value::<TransferFileInput>(invalid).is_err());
    }
}
