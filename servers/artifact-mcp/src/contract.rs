//! Typed MCP contract for artifact discovery, authorization, and sharing.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub use veoveo_artifact_contract::{
    ArtifactId, ArtifactMetadata, ArtifactMetadataSnapshot, ArtifactReleaseState,
    ArtifactShareLink, ArtifactShareLinkId, Grant,
};

use veoveo_types::AccessLevel;
use veoveo_types::AccessSubject;

pub const INDEX_URI: &str = "artifact://index";
pub const INDEX_TEMPLATE: &str = "artifact://index{?cursor}";
pub const LIBRARY_APP_URI: &str = "ui://artifact/library.html";
pub const ARTIFACT_TEMPLATE: &str = "artifact://{artifact_id}";
pub const METADATA_TEMPLATE: &str = "artifact://metadata/{artifact_id}";
pub const GRANTS_TEMPLATE: &str = "artifact://grants/{artifact_id}";

/// Well-known surface roots (contract C18, C19). These literals must match
/// the Artifact resource builders.
pub const DOCS_URI: &str = "artifact://docs";
pub const CONTRACT_URI: &str = "artifact://contract";
pub const DOC_TEMPLATE: &str = "artifact://docs/{doc_id}";

mod index;
pub use index::{ArtifactIndexCursor, ArtifactIndexEntry, ArtifactIndexPage};
mod resources;
mod scopes;
pub use resources::{
    ArtifactDocument, ArtifactResource, doc_uri, grants_uri, metadata_uri, parse_doc_uri,
    parse_grants_uri, parse_metadata_uri,
};
pub use scopes::ArtifactScope;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReference {
    pub artifact_id: ArtifactId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrantArtifactRequest {
    pub artifact_id: ArtifactId,
    pub subject: AccessSubject,
    pub level: AccessLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevokeArtifactGrantRequest {
    pub artifact_id: ArtifactId,
    pub subject: AccessSubject,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetArtifactReleaseRequest {
    pub artifact_id: ArtifactId,
    pub release_state: ArtifactReleaseState,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShareLinkOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<veoveo_types::ChronoUtcTimestampSchema>")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_downloads: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(from = "CreateArtifactShareWire")]
pub struct CreateArtifactShareRequest {
    pub artifact_id: ArtifactId,
    #[serde(flatten)]
    pub options: ShareLinkOptions,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CreateArtifactShareWire {
    artifact_id: ArtifactId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<veoveo_types::ChronoUtcTimestampSchema>")]
    expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_downloads: Option<u64>,
}

impl From<CreateArtifactShareWire> for CreateArtifactShareRequest {
    fn from(wire: CreateArtifactShareWire) -> Self {
        Self {
            artifact_id: wire.artifact_id,
            options: ShareLinkOptions {
                expires_at: wire.expires_at,
                max_downloads: wire.max_downloads,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevokeArtifactShareRequest {
    pub artifact_id: ArtifactId,
    pub link_id: ArtifactShareLinkId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactMetadataOutput {
    pub artifact: ArtifactMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactGrantsOutput {
    pub artifact_id: ArtifactId,
    pub grants: Vec<Grant>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactShareOutput {
    pub share_link: ArtifactShareLink,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactMutationOutput {
    pub artifact_id: ArtifactId,
    pub changed: bool,
}

#[cfg(test)]
mod input_strictness_tests {
    use super::*;
    use serde_json::json;

    fn rejects_extra<T: serde::de::DeserializeOwned>(mut input: serde_json::Value) {
        assert!(serde_json::from_value::<T>(input.clone()).is_ok());
        input["undeclared"] = json!(true);
        let error = serde_json::from_value::<T>(input).err().unwrap();
        assert!(error.to_string().contains("undeclared"));
    }

    #[test]
    fn sharing_timestamp_schema_and_decoder_keep_optional_current_profile() {
        let id = ArtifactId::new();
        let schemas = json!({
            "ShareLinkOptions": schemars::schema_for!(ShareLinkOptions),
            "CreateArtifactShareRequest": schemars::schema_for!(CreateArtifactShareRequest),
        });
        let mut cases = Vec::new();
        for timestamp in [
            chrono::DateTime::<Utc>::MIN_UTC,
            chrono::DateTime::<Utc>::MAX_UTC,
            "2016-12-31T23:59:60.123456789Z".parse().unwrap(),
            "2026-10-05T14:34:56.123456789+02:00".parse().unwrap(),
        ] {
            let options = ShareLinkOptions {
                expires_at: Some(timestamp),
                max_downloads: Some(3),
            };
            let options_wire = serde_json::to_value(&options).unwrap();
            let request = CreateArtifactShareRequest {
                artifact_id: id,
                options,
            };
            let request_wire = serde_json::to_value(&request).unwrap();
            let decoded: CreateArtifactShareRequest =
                serde_json::from_value(request_wire.clone()).unwrap();
            assert_eq!(decoded.options.expires_at, Some(timestamp));
            assert_eq!(serde_json::to_value(decoded).unwrap(), request_wire);
            let decoded: ShareLinkOptions = serde_json::from_value(options_wire.clone()).unwrap();
            assert_eq!(decoded.expires_at, Some(timestamp));
            cases.push(json!({"options": options_wire, "request":request_wire}));
        }
        for wire in [
            json!({"artifact_id":id}),
            json!({"artifact_id":id,"expires_at":null}),
        ] {
            let decoded: CreateArtifactShareRequest = serde_json::from_value(wire.clone()).unwrap();
            assert_eq!(decoded.options.expires_at, None);
            assert!(
                serde_json::to_value(decoded)
                    .unwrap()
                    .get("expires_at")
                    .is_none()
            );
            cases.push(json!({"request":wire}));
        }
        for malformed in [
            json!(true),
            json!(1),
            json!("2026-02-30T00:00:00Z"),
            json!("2026-01-01T00:00:00"),
            json!("+262143-01-01T00:00:00Z"),
        ] {
            assert!(
                serde_json::from_value::<CreateArtifactShareRequest>(json!({
                    "artifact_id":id,"expires_at":malformed
                }))
                .is_err()
            );
        }
        assert!(
            serde_json::from_value::<CreateArtifactShareRequest>(json!({
                "artifact_id":id,"expiresAt":"2026-01-01T00:00:00Z"
            }))
            .is_err()
        );
        for schema in schemas.as_object().unwrap().values() {
            let field = &schema["properties"]["expires_at"];
            assert!(field.is_object());
            assert!(!serde_json::to_string(field).unwrap().contains("date-time"));
            assert!(
                !schema["required"]
                    .as_array()
                    .is_some_and(|fields| fields.iter().any(|field| field == "expires_at"))
            );
        }
        // The existing owner test exposes actual schema/typed producer observations
        // for the maintained browser validator; capture does not assert browser admission.
        if let Some(path) = std::env::var_os("VEOVEO_ARTIFACT_SHARE_TIMESTAMP_SCHEMA_CAPTURE") {
            std::fs::write(
                path,
                serde_json::to_vec_pretty(&json!({"schemas":schemas,"cases":cases})).unwrap(),
            )
            .unwrap();
        }
    }

    #[test]
    fn tool_inputs_reject_extra_keys_and_sharing_keeps_its_flat_wire() {
        let id = ArtifactId::new();
        let subject = json!({"kind":"group","id":"engineering"});
        rejects_extra::<ArtifactReference>(json!({"artifact_id":id}));
        rejects_extra::<GrantArtifactRequest>(
            json!({"artifact_id":id,"subject":subject,"level":"read"}),
        );
        rejects_extra::<RevokeArtifactGrantRequest>(json!({"artifact_id":id,"subject":subject}));
        rejects_extra::<SetArtifactReleaseRequest>(
            json!({"artifact_id":id,"release_state":"private"}),
        );
        rejects_extra::<RevokeArtifactShareRequest>(
            json!({"artifact_id":id,"link_id":ArtifactShareLinkId::new()}),
        );
        rejects_extra::<CreateArtifactShareRequest>(json!({"artifact_id":id}));
        let wire = json!({"artifact_id":id,"expires_at":"2030-01-01T00:00:00Z","max_downloads":3});
        let request: CreateArtifactShareRequest = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(request.options.max_downloads, Some(3));
        assert_eq!(serde_json::to_value(request).unwrap(), wire);
        rejects_extra::<CreateArtifactShareRequest>(wire);
        let schema =
            serde_json::to_value(schemars::schema_for!(CreateArtifactShareRequest)).unwrap();
        assert_eq!(schema["additionalProperties"], false);
        rejects_extra::<ShareLinkOptions>(json!({"max_downloads":3}));
    }
}
