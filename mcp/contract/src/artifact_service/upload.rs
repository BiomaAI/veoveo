//! Public resumable HTTP upload contract. File bytes never enter MCP messages.

use super::*;

mod policy;
pub use policy::*;

artifact_uuid_id!(ArtifactUploadId, "artifact upload id");
artifact_uuid_id!(ArtifactUploadRequestId, "upload idempotency key");

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
    pub artifact_uri: String,
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
