/// Canonical identity of one logical artifact occurrence. Every put creates a
/// fresh UUIDv7 even when its bytes deduplicate to an existing tenant blob.
#[veoveo_types::id(uuid(ArtifactIds), fresh)]
pub struct ArtifactId(uuid::Uuid);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("artifact id must be a UUIDv7")]
pub struct ArtifactIdError;

impl ArtifactId {
    pub fn plane_uri(self) -> crate::ArtifactUri {
        crate::ArtifactUri::plane(self)
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

use veoveo_types::{
    FreshId, IdGeneration, IdProfile, IdProfileSpec, IdSchema, UuidGrammar, UuidSpelling,
    UuidVariant,
};

#[doc(hidden)]
pub struct ArtifactIds;
impl IdProfile for ArtifactIds {
    type Error = ArtifactIdError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        generation: IdGeneration {
            fresh: FreshId::UuidV7,
            stable_v5_namespace: None,
        },
        schema: IdSchema::DerivedString,
        ..IdProfileSpec::uuid(
            UuidGrammar {
                versions: &[7],
                variant: UuidVariant::Rfc4122,
                spelling: UuidSpelling::ParserAliases,
            },
            |_, _, _| ArtifactIdError,
        )
    };
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
