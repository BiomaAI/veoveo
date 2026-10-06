//! Pure Artifact-plane control and byte-transfer descriptors.
use crate::*;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeSet, num::NonZeroU64};
use veoveo_types::{AccessLevel, AccessSubject, DataLabelId};

/// Maximum serialized put descriptor size.
pub const MAX_ARTIFACT_PUT_DESCRIPTOR_BYTES: usize = 4 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetArtifactReleaseStateRequest {
    pub release_state: ArtifactReleaseState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateArtifactShareLinkRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_downloads: Option<NonZeroU64>,
}

/// Presentation and client-declared sensitivity for a new artifact.
///
/// Deliberately has no `tenant_id` or `owner_id`: those are stamped by the
/// service from the verified identity and can never be asserted by the client.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PutArtifactRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// Client-declared classification, unioned into the artifact's labels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classification: Option<DataLabelId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub data_labels: BTreeSet<DataLabelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

impl PutArtifactRequest {
    /// The full label set the artifact carries: classification unioned with the
    /// explicit data labels. This is what MAC is evaluated against.
    pub fn effective_labels(&self) -> BTreeSet<DataLabelId> {
        let mut labels = self.data_labels.clone();
        if let Some(class) = &self.classification {
            labels.insert(class.clone());
        }
        labels
    }
}

/// Internal bounded streaming upload descriptor.
///
/// The expected length and digest are mandatory because the service must reject
/// a corrupt or truncated body before committing an object or occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StreamArtifactRequest {
    /// Pre-reserved occurrence identity. Recording publication uses the
    /// distinct `RecordingLayerId` wrapper around this same UUIDv7.
    pub artifact_id: ArtifactId,
    pub artifact: PutArtifactRequest,
    pub expected_byte_len: u64,
    pub expected_sha256: UploadSha256,
}

/// A grant mutation request. The occurrence id travels in the request path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PutGrantRequest {
    pub subject: AccessSubject,
    pub level: AccessLevel,
}

/// The grants recorded for one artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GrantList {
    pub grants: Vec<Grant>,
}

/// Keyset-paginated discovery request for artifacts visible to the caller.
///
/// The cursor is an opaque occurrence identity, not a content hash or storage
/// key. Implementations must still apply the complete artifact policy to every
/// returned item.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListArtifactsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<ArtifactId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactPage {
    pub artifacts: Vec<ArtifactMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<ArtifactId>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::{TimeDelta, Utc};
    use serde_json::json;

    use super::*;
    use veoveo_types::DataLabelId;

    #[test]
    fn output_capability_has_one_closed_optional_label_floor() {
        let base = json!({"task_id":uuid::Uuid::now_v7(),"expires_at":chrono::DateTime::parse_from_rfc3339("2026-10-01T00:00:00Z").unwrap().with_timezone(&Utc)+TimeDelta::minutes(5),"max_artifact_count":2,"max_total_bytes":1024});
        let request: IssueArtifactWriteCapabilityRequest =
            serde_json::from_value(base.clone()).unwrap();
        assert!(request.required_data_labels.is_empty());
        let mut constrained = base.clone();
        constrained["required_data_labels"] = json!(["retained-home"]);
        let request: IssueArtifactWriteCapabilityRequest =
            serde_json::from_value(constrained).unwrap();
        assert_eq!(
            request.required_data_labels,
            BTreeSet::from([DataLabelId::parse("retained-home").unwrap()])
        );
        let mut typo = base;
        typo["required_labels"] = json!(["retained-home"]);
        assert!(serde_json::from_value::<IssueArtifactWriteCapabilityRequest>(typo).is_err());
    }

    #[test]
    fn occurrence_and_control_ids_are_uuid_v7() {
        let artifact = ArtifactId::new();
        let capability = ArtifactWriteCapabilityId::new();
        let link = ArtifactShareLinkId::new();
        let access_request = ArtifactAccessRequestId::new();

        assert_eq!(artifact.as_uuid().get_version_num(), 7);
        assert_eq!(capability.as_uuid().get_version_num(), 7);
        assert_eq!(link.as_uuid().get_version_num(), 7);
        assert_eq!(access_request.as_uuid().get_version_num(), 7);
        assert_eq!(ArtifactId::parse(artifact.to_string()).unwrap(), artifact);
        assert_eq!(
            ArtifactAccessRequestId::parse(access_request.to_string()).unwrap(),
            access_request
        );
        let uuid_v4 = uuid::Uuid::parse_str("67e55044-10b1-426f-9247-bb680e5fe0c8").unwrap();
        assert_eq!(uuid_v4.get_version_num(), 4);
        assert_eq!(uuid_v4.get_variant(), uuid::Variant::RFC4122);
        assert!(ArtifactId::parse(uuid_v4.to_string()).is_err());
        assert!(ArtifactWriteCapabilityId::parse("not-a-uuid").is_err());
    }

    #[test]
    fn stream_descriptor_uses_the_owner_bare_lowercase_digest() {
        let descriptor = StreamArtifactRequest {
            artifact_id: ArtifactId::new(),
            artifact: PutArtifactRequest::default(),
            expected_byte_len: 0,
            expected_sha256: UploadSha256::parse("ab".repeat(32)).unwrap(),
        };
        let schema = serde_json::to_value(schemars::schema_for!(StreamArtifactRequest)).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let wire = serde_json::to_value(&descriptor).unwrap();
        assert_eq!(wire["expected_sha256"], "ab".repeat(32));
        assert!(validator.is_valid(&wire));
        assert_eq!(
            serde_json::from_value::<StreamArtifactRequest>(wire.clone()).unwrap(),
            descriptor
        );
        for malformed in [
            "AB".repeat(32),
            format!("sha256:{}", "ab".repeat(32)),
            "00".repeat(31),
        ] {
            let mut changed = wire.clone();
            changed["expected_sha256"] = serde_json::json!(malformed);
            assert!(!validator.is_valid(&changed));
            assert!(serde_json::from_value::<StreamArtifactRequest>(changed).is_err());
        }
    }

    #[test]
    fn artifact_write_idempotency_key_is_validated() {
        let key = ArtifactWriteIdempotencyKey::new("media:task:output:0").unwrap();
        assert_eq!(key.as_str(), "media:task:output:0");
        assert!(ArtifactWriteIdempotencyKey::new("").is_err());
        assert!(ArtifactWriteIdempotencyKey::new(" leading").is_err());
        assert!(ArtifactWriteIdempotencyKey::new("line\nbreak").is_err());
        assert!(ArtifactWriteIdempotencyKey::new("x".repeat(257)).is_err());
    }

    #[test]
    fn effective_labels_union_classification() {
        let classification = DataLabelId::parse("cui").unwrap();
        let explicit = DataLabelId::parse("us_only").unwrap();
        let request = PutArtifactRequest {
            classification: Some(classification.clone()),
            data_labels: BTreeSet::from([explicit.clone()]),
            metadata: json!({"producer": "test"}),
            ..Default::default()
        };
        assert_eq!(
            request.effective_labels(),
            BTreeSet::from([classification, explicit])
        );
    }

    #[test]
    fn capability_secret_is_validated_and_redacted() {
        assert!(ArtifactWriteCapabilitySecret::new("short").is_err());
        let secret = ArtifactWriteCapabilitySecret::new("s".repeat(32)).unwrap();
        assert_eq!(
            format!("{secret:?}"),
            "ArtifactWriteCapabilitySecret(<redacted>)"
        );
        let issued = IssuedArtifactWriteCapability {
            capability_id: ArtifactWriteCapabilityId::new(),
            secret,
            task_id: ArtifactTaskId::new(),
            expires_at: chrono::DateTime::parse_from_rfc3339("2026-10-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
                + TimeDelta::minutes(5),
        };
        assert!(!format!("{issued:?}").contains(&"s".repeat(32)));
    }

    #[test]
    fn share_link_defaults_do_not_assert_limits() {
        let request = CreateArtifactShareLinkRequest::default();
        assert!(request.expires_at.is_none());
        assert!(request.max_downloads.is_none());
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value, json!({}));
    }

    #[test]
    fn share_link_debug_redacts_the_bearer_url() {
        let token = "public-share-token-that-must-not-be-logged";
        let link = ArtifactShareLink {
            link_id: ArtifactShareLinkId::new(),
            artifact_id: ArtifactId::new(),
            url: format!("https://veoveo.example/s/{token}"),
            expires_at: chrono::DateTime::parse_from_rfc3339("2026-10-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
                + TimeDelta::minutes(5),
            max_downloads: None,
        };
        let diagnostic = format!("{link:?}");
        assert!(!diagnostic.contains(token));
        assert!(diagnostic.contains("<redacted>"));
    }
}
