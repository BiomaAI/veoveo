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
