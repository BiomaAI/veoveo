use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Canonical identity of one logical artifact occurrence. Every put creates a
/// fresh UUIDv7 even when its bytes deduplicate to an existing tenant blob.
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(error = ArtifactIdError, admit = admit_artifact_id, constructor = parse, wire_string, generate = uuid::Uuid::now_v7)]
pub struct ArtifactId(uuid::Uuid);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactIdError;

impl std::error::Error for ArtifactIdError {}

impl ArtifactId {
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0
    }

    pub fn plane_uri(self) -> crate::ArtifactUri {
        crate::ArtifactUri::plane(self)
    }
}

impl TryFrom<uuid::Uuid> for ArtifactId {
    type Error = ArtifactIdError;

    fn try_from(value: uuid::Uuid) -> Result<Self, Self::Error> {
        if value.get_version_num() != 7 || value.get_variant() != uuid::Variant::RFC4122 {
            return Err(ArtifactIdError);
        }
        Ok(Self(value))
    }
}

/// The neutral scheme every server uses to name any artifact on the shared plane.
pub const ARTIFACT_PLANE_SCHEME: &str = "artifact";

/// Parse the canonical occurrence identity from either `artifact://{id}` or a
/// server presentation such as `media://artifact/{id}`.
pub fn parse_artifact_plane_uri(uri: &str) -> Option<ArtifactId> {
    crate::ArtifactUri::parse(uri)
        .ok()
        .map(|uri| uri.artifact_id())
}

fn admit_artifact_id(value: &str) -> Result<uuid::Uuid, ArtifactIdError> {
    let uuid = uuid::Uuid::parse_str(value).map_err(|_| ArtifactIdError)?;
    ArtifactId::try_from(uuid).map(|id| id.0)
}

impl std::fmt::Display for ArtifactIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("artifact id must be a UUIDv7")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plane_uri_accepts_neutral_and_server_forms() {
        let expected = ArtifactId::new();
        // Neutral plane form.
        assert_eq!(
            parse_artifact_plane_uri(&format!("artifact://{expected}")),
            Some(expected)
        );
        // Any server-presented `{scheme}://artifact/{artifact_id}` form round-trips.
        for scheme in ["media", "duckdb", "timeseries", "optimization"] {
            assert_eq!(
                parse_artifact_plane_uri(&format!("{scheme}://artifact/{expected}")),
                Some(expected),
                "scheme {scheme}"
            );
        }
        // Junk and non-UUIDv7 ids are rejected.
        assert_eq!(parse_artifact_plane_uri("artifact://not-an-id"), None);
        assert_eq!(parse_artifact_plane_uri("media://artifact/xyz"), None);
        assert_eq!(parse_artifact_plane_uri("media://models"), None);
    }

    #[test]
    fn occurrence_uri_is_uuid_v7() {
        let artifact_id = ArtifactId::new();
        let parsed = parse_artifact_plane_uri(artifact_id.plane_uri().as_str()).unwrap();
        assert_eq!(parsed, artifact_id);
        assert!(parse_artifact_plane_uri("artifact://bad").is_none());
        assert!(parse_artifact_plane_uri("media://artifact/x").is_none());
    }
}
