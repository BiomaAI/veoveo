use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Canonical identity of one logical artifact occurrence. Every put creates a
/// fresh UUIDv7 even when its bytes deduplicate to an existing tenant blob.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct ArtifactId(uuid::Uuid);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactIdError;

impl std::fmt::Display for ArtifactIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("artifact id must be a UUIDv7")
    }
}

impl std::error::Error for ArtifactIdError {}

impl ArtifactId {
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, ArtifactIdError> {
        let value = uuid::Uuid::parse_str(value.as_ref()).map_err(|_| ArtifactIdError)?;
        Self::try_from(value)
    }

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

impl Default for ArtifactId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ArtifactId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<String> for ArtifactId {
    type Error = ArtifactIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl std::str::FromStr for ArtifactId {
    type Err = ArtifactIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl From<ArtifactId> for String {
    fn from(value: ArtifactId) -> Self {
        value.to_string()
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
