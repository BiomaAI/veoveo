use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types as surrealdb_types;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

use crate::{InvocationAuthorityRecord, PrincipalKind};

/// Whole profile document with its Gateway-owned admitted policy lookup.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactUploadProfile {
    pub document: crate::OpenObject,
    pub profile_policy_version: veoveo_types::PolicyVersion,
}
impl SurrealValue for ArtifactUploadProfile {
    fn kind_of() -> surrealdb::types::Kind {
        surrealdb::types::Kind::Object
    }
    fn is_value(value: &surrealdb::types::Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> surrealdb::types::Value {
        crate::json_value::into_surreal(serde_json::to_value(self).expect("typed upload profile"))
    }
    fn from_value(value: surrealdb::types::Value) -> Result<Self, surrealdb::types::Error> {
        serde_json::from_value(crate::json_value::from_surreal_json(value)?).map_err(|_| {
            surrealdb::types::Error::internal("invalid upload profile metadata".into())
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactUploadAuthorityVersion {
    pub control_plane_sha256: String,
    pub policy_revision: String,
    pub context_digest: String,
    pub profile_policy_digest: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactUploadRejection {
    Conflict,
    Denied,
    Quota,
    Busy,
    Expired,
    Integrity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
#[surreal(untagged)]
pub enum ArtifactUploadState {
    #[surreal(value = "open")]
    Open,
    #[surreal(value = "finalizing")]
    Finalizing,
    #[surreal(value = "verifying")]
    Verifying,
    #[surreal(value = "completed")]
    Completed,
    #[surreal(value = "cancelled")]
    Cancelled,
    #[surreal(value = "expired")]
    Expired,
    #[surreal(value = "failed")]
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
#[surreal(untagged)]
pub enum ArtifactUploadPartState {
    #[surreal(value = "reserved")]
    Reserved,
    #[surreal(value = "accepted")]
    Accepted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
#[surreal(untagged)]
pub enum ArtifactUploadFailure {
    #[surreal(value = "integrity")]
    Integrity,
    #[surreal(value = "authority_changed")]
    AuthorityChanged,
    #[surreal(value = "policy_changed")]
    PolicyChanged,
    #[surreal(value = "storage")]
    Storage,
    #[surreal(value = "expired")]
    Expired,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactUploadDescriptor {
    pub filename: String,
    pub mime_type: String,
    pub byte_len: Option<i64>,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactUploadLayout {
    pub part_bytes: i64,
    pub max_parts: i64,
    pub max_total_bytes: i64,
    pub parallel_parts: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactUploadManifest {
    pub byte_len: i64,
    pub part_count: i64,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactUploadRecord {
    pub audit: crate::audit::AuditContextRecord,
    pub id: RecordId,
    pub tenant: RecordId,
    pub tenant_key: String,
    pub actor: RecordId,
    pub actor_key: String,
    pub actor_kind: PrincipalKind,
    pub actor_issuer: String,
    pub actor_subject: String,
    pub profile_key: String,
    pub work_context: RecordId,
    pub authority: InvocationAuthorityRecord,
    pub context_digest: String,
    pub policy_digest: String,
    pub profile_policy_digest: String,
    pub request_id: Uuid,
    pub descriptor: ArtifactUploadDescriptor,
    pub layout: ArtifactUploadLayout,
    pub state: ArtifactUploadState,
    pub reserved_bytes: i64,
    pub accepted_bytes: i64,
    pub accepted_part_count: i64,
    pub object_key: String,
    pub multipart_id: Option<String>,
    pub generation: i64,
    pub lease_owner: Option<Uuid>,
    pub lease_until: Option<DateTime<Utc>>,
    pub manifest: Option<ArtifactUploadManifest>,
    pub artifact: RecordId,
    pub verified_sha256: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
    pub failure: Option<ArtifactUploadFailure>,
    pub cleanup_pending: bool,
    pub cleanup_bytes: i64,
    pub inactivity_seconds: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub lifetime_ends_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactUploadPartRecord {
    pub id: RecordId,
    pub upload: RecordId,
    pub tenant: RecordId,
    pub part_number: i64,
    pub byte_len: i64,
    pub sha256: String,
    pub state: ArtifactUploadPartState,
    pub content_id: Option<String>,
    pub generation: i64,
    pub lease_owner: Option<Uuid>,
    pub lease_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactStorageUsage {
    pub id: RecordId,
    pub tenant: RecordId,
    pub committed_bytes: i64,
    pub reserved_bytes: i64,
    pub cleanup_bytes: i64,
    pub active_uploads: i64,
    pub inflight_bytes: i64,
    pub inflight_parts: i64,
}

pub fn upload_record_id(id: Uuid) -> RecordId {
    RecordId::new("artifact_upload", surrealdb::types::Uuid::from(id))
}

pub fn upload_part_record_id(id: Uuid, part_number: u32) -> RecordId {
    let key = Uuid::new_v5(&id, &part_number.to_be_bytes());
    RecordId::new("artifact_upload_part", surrealdb::types::Uuid::from(key))
}

pub fn artifact_storage_usage_id(tenant: crate::TenantId) -> RecordId {
    RecordId::new(
        "artifact_storage_usage",
        surrealdb::types::Uuid::from(tenant.as_uuid()),
    )
}

#[cfg(test)]
mod profile_tests {
    use super::*;
    #[test]
    fn profile_metadata_preserves_json_and_rejects_native_control_values() {
        let profile = ArtifactUploadProfile {
            document: crate::OpenObject::new(std::collections::BTreeMap::from([
                ("policy_version".into(), serde_json::json!("r1")),
                (
                    "metadata".into(),
                    serde_json::json!({"number": u64::MAX, "null": null}),
                ),
            ])),
            profile_policy_version: "r1".parse().unwrap(),
        };
        assert_eq!(
            ArtifactUploadProfile::from_value(profile.clone().into_value()).unwrap(),
            profile
        );
        let surrealdb::types::Value::Object(mut invalid) = profile.into_value() else {
            panic!("profile object")
        };
        let surrealdb::types::Value::Object(document) = invalid.get_mut("document").unwrap() else {
            panic!("profile document")
        };
        document.insert("policy_version", RecordId::new("policy", "r1").into_value());
        assert!(ArtifactUploadProfile::from_value(invalid.into_value()).is_err());
    }
}
