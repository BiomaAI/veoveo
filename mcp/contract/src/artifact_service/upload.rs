//! Public resumable HTTP upload contract. File bytes never enter MCP messages.

use super::*;

mod policy;
pub use policy::*;

pub use veoveo_artifact_contract::{ArtifactUploadId, ArtifactUploadRequestId};

pub const UPLOAD_PART_BYTE_LEN_HEADER: &str = "x-veoveo-part-byte-len";
pub const UPLOAD_PART_SHA256_HEADER: &str = "x-veoveo-part-sha256";
pub const ARTIFACT_UPLOAD_AUDIENCE: &str = "artifact-upload";
pub const UPLOAD_PART_PAGE_LIMIT: usize = 256;
/// JSON counters also travel through browser numbers. Reject lossy conversions.
pub const MAX_UPLOAD_BYTES: u64 = (1_u64 << 53) - 1;

/// Ordinary SHA-256 in lowercase hexadecimal, never a multipart/composite ETag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UploadSha256(String);

impl JsonSchema for UploadSha256 {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "UploadSha256".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
    }
}

impl UploadSha256 {
    pub fn parse(value: impl Into<String>) -> Result<Self, ArtifactPlaneError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ArtifactPlaneError::InvalidRequest(
                "sha256 must contain 64 lowercase hexadecimal characters".into(),
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for UploadSha256 {
    type Error = ArtifactPlaneError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<UploadSha256> for String {
    fn from(value: UploadSha256) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateArtifactUpload {
    pub filename: String,
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_len: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<UploadSha256>,
}

impl CreateArtifactUpload {
    pub fn validate(&self) -> Result<(), UploadErrorCode> {
        if self.filename.is_empty()
            || self.filename.len() > 255
            || self.filename.trim() != self.filename
            || matches!(self.filename.as_str(), "." | "..")
            || self
                .filename
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\'))
            || !valid_upload_mime_type(&self.mime_type)
            || self.byte_len.is_some_and(|n| n > MAX_UPLOAD_BYTES)
        {
            return Err(UploadErrorCode::Malformed);
        }
        Ok(())
    }
}

/// MIME essence only: parameters and header syntax do not belong in descriptors.
pub fn valid_upload_mime_type(value: &str) -> bool {
    let Some((kind, subtype)) = value.split_once('/') else {
        return false;
    };
    !kind.is_empty()
        && !subtype.is_empty()
        && value.len() <= 127
        && [kind, subtype].iter().all(|part| {
            part.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"!#$&^_.+-".contains(&b))
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactUploadState {
    Open,
    Finalizing,
    Verifying,
    Completed,
    Cancelled,
    Expired,
    Failed,
}

impl ArtifactUploadState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Cancelled | Self::Expired | Self::Failed
        )
    }
}

/// Upload notifications trigger a currently authorized status read.
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum ArtifactUploadNotificationState {
    #[vocabulary(rename = "finalizing")]
    Finalizing,
    #[vocabulary(rename = "verifying")]
    Verifying,
    #[vocabulary(rename = "completed")]
    Completed,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
    #[vocabulary(rename = "expired")]
    Expired,
    #[vocabulary(rename = "failed")]
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArtifactUploadNotification {
    Changed {
        // Notifications emit the canonical ID spelling; other ledger profiles retain their aliases.
        #[schemars(schema_with = "notification_upload_id_schema")]
        upload_id: ArtifactUploadId,
        state: ArtifactUploadNotificationState,
    },
}

fn notification_upload_id_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    // The emitted notification profile is canonical; ledger admission keeps aliases.
    schemars::json_schema!({
        "type": "string",
        "pattern": "^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UploadPartReceipt {
    pub part_number: NonZeroU32,
    pub byte_len: u64,
    pub sha256: UploadSha256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompleteArtifactUpload {
    pub byte_len: u64,
    pub part_count: NonZeroU32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<UploadSha256>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactUploadReceipt {
    pub upload_id: ArtifactUploadId,
    pub artifact_id: ArtifactId,
    pub artifact_uri: veoveo_artifact_contract::ArtifactUri,
    pub sha256: UploadSha256,
    pub byte_len: u64,
    pub mime_type: String,
    pub filename: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactUploadSession {
    pub upload_id: ArtifactUploadId,
    pub state: ArtifactUploadState,
    pub descriptor: CreateArtifactUpload,
    pub layout: UploadLayout,
    pub accepted_bytes: u64,
    pub accepted_part_count: u32,
    pub parts: Vec<UploadPartReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_part_cursor: Option<NonZeroU32>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<ArtifactUploadReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<UploadErrorCode>,
}

/// Gateway-only envelope. Public callers never select authority or policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactUploadAuthority {
    pub control_plane_sha256: UploadSha256,
    pub context_digest: UploadSha256,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UploadErrorCode {
    Malformed,
    Unauthenticated,
    Denied,
    NotFound,
    Conflict,
    Expired,
    TooLarge,
    UnsupportedType,
    Integrity,
    QuotaExceeded,
    Busy,
    Unavailable,
}

impl UploadErrorCode {
    pub fn http_status(self) -> u16 {
        match self {
            Self::Malformed => 400,
            Self::Unauthenticated => 401,
            Self::Denied => 403,
            Self::NotFound => 404,
            Self::Conflict => 409,
            Self::Expired => 410,
            Self::TooLarge => 413,
            Self::UnsupportedType => 415,
            Self::Integrity => 422,
            Self::QuotaExceeded | Self::Busy => 429,
            Self::Unavailable => 503,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactUploadError {
    pub code: UploadErrorCode,
    /// Safe, bounded operator-independent copy; never a backend error string.
    pub message: String,
    pub request_id: ArtifactUploadRequestId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_bytes: Option<u64>,
}

/// Source-owner Artifact transfer and access contracts consumed by browser clients.
#[derive(JsonSchema)]
#[expect(dead_code, reason = "schema-only bundle selects owner contracts")]
struct ArtifactTransferSchema {
    notification: ArtifactUploadNotification,
    descriptor: CreateArtifactUpload,
    receipt: ArtifactUploadReceipt,
    session: ArtifactUploadSession,
    part: UploadPartReceipt,
    policy: EffectiveArtifactUploadPolicy,
    access_request: ArtifactAccessRequest,
    access_request_page: ArtifactAccessRequestPage,
    share_link: veoveo_artifact_contract::ArtifactShareLink,
}

pub fn schema_bundle() -> schemars::Schema {
    schemars::schema_for!(ArtifactTransferSchema)
}

#[cfg(test)]
mod notification_tests {
    use super::*;

    #[test]
    fn accepted_aliases_emit_the_canonical_shared_notification() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../testdata/upload-notifications.json")).unwrap();
        let notification: ArtifactUploadNotification =
            serde_json::from_value(fixture["alias"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(notification).unwrap(),
            fixture["emitted"]
        );
        for id in fixture["invalid_ids"].as_array().unwrap() {
            let mut wire = fixture["emitted"].clone();
            wire["upload_id"] = id.clone();
            assert!(serde_json::from_value::<ArtifactUploadNotification>(wire).is_err());
        }
    }

    #[test]
    fn emitted_upload_states_preserve_snake_case_contentless_notifications() {
        let upload_id = ArtifactUploadId::new();
        for state in ArtifactUploadNotificationState::ALL {
            let value = ArtifactUploadNotification::Changed {
                upload_id,
                state: *state,
            };
            let wire = serde_json::to_value(&value).unwrap();
            assert_eq!(wire["op"], "changed");
            assert_eq!(wire["upload_id"], upload_id.to_string());
            assert_eq!(wire["state"], state.as_str());
            assert_eq!(wire.as_object().unwrap().len(), 3);
            assert!(serde_json::from_value::<ArtifactUploadNotification>(wire).is_ok());
        }
        for wire in [
            serde_json::json!({"op":"changed","upload_id":upload_id,"state":"open"}),
            serde_json::json!({"op":"changed","upload_id":upload_id,"state":"completed","receipt":{}}),
            serde_json::json!({"op":"unknown","upload_id":upload_id,"state":"completed"}),
            serde_json::json!({"op":"changed","upload_id":"invalid","state":"completed"}),
        ] {
            assert!(serde_json::from_value::<ArtifactUploadNotification>(wire).is_err());
        }
    }
}
