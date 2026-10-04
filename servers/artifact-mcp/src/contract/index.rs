//! The metadata collection owns its cursor and URI page contract.
use super::{ArtifactId, ArtifactMetadata, metadata_uri};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceUri, ResourceUriError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct ArtifactIndexCursor(ArtifactId);

impl ArtifactIndexCursor {
    pub fn new(after: ArtifactId) -> Self {
        Self(after)
    }
    pub fn after(self) -> ArtifactId {
        self.0
    }
    pub fn parse(value: &str) -> Result<Self, ResourceUriError> {
        use veoveo_types::CursorCodec;
        let id = ArtifactIndexCodec.decode(value)?;
        ArtifactIndexCodec.check(&id)?;
        Ok(Self(id))
    }
}
impl TryFrom<String> for ArtifactIndexCursor {
    type Error = ResourceUriError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}
impl From<ArtifactIndexCursor> for String {
    fn from(cursor: ArtifactIndexCursor) -> Self {
        use veoveo_types::CursorCodec;
        ArtifactIndexCodec
            .encode(&cursor.0)
            .expect("typed Artifact index position")
    }
}
// The published Copy representation owns only its ID; wire is computed at conversion.
struct ArtifactIndexCodec;
impl veoveo_types::CursorCodec for ArtifactIndexCodec {
    type Position = ArtifactId;
    type Error = ResourceUriError;
    fn check(&self, _id: &ArtifactId) -> Result<(), Self::Error> {
        Ok(())
    }
    fn encode(&self, id: &ArtifactId) -> Result<String, Self::Error> {
        Ok(format!("artifact-index-v1_{id}"))
    }
    fn decode(&self, wire: &str) -> Result<ArtifactId, Self::Error> {
        let id = wire
            .strip_prefix("artifact-index-v1_")
            .ok_or(ResourceUriError::DisallowedComponent)?;
        let id = ArtifactId::parse(id).map_err(|_| ResourceUriError::DisallowedComponent)?;
        if self.encode(&id)? != wire {
            return Err(ResourceUriError::DisallowedComponent);
        }
        Ok(id)
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactIndexEntry {
    uri: ResourceUri,
    title: String,
    mime_type: &'static str,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactIndexPage {
    items: Vec<ArtifactIndexEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<ArtifactIndexCursor>,
}
impl ArtifactIndexPage {
    pub fn new(
        artifacts: Vec<ArtifactMetadata>,
        next_cursor: Option<ArtifactId>,
    ) -> Result<Self, ResourceUriError> {
        if artifacts.len() > 100 || (artifacts.is_empty() && next_cursor.is_some()) {
            return Err(ResourceUriError::DisallowedComponent);
        }
        Ok(Self {
            items: artifacts
                .into_iter()
                .map(|artifact| {
                    let id = artifact.artifact_id();
                    ArtifactIndexEntry {
                        uri: metadata_uri(id),
                        title: artifact.filename.unwrap_or_else(|| id.to_string()),
                        mime_type: "application/json",
                    }
                })
                .collect(),
            next_cursor: next_cursor.map(ArtifactIndexCursor::new),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pages_are_bounded_metadata_links_without_transfer_locations() {
        let id = ArtifactId::new();
        let artifact: ArtifactMetadata = serde_json::from_value(serde_json::json!({
            "artifact_id": id, "artifact_uri": id.plane_uri(), "byte_len": 4,
            "filename": "report.txt", "mime_type": "text/plain",
            "download_url": "https://example.test/private-transfer",
            "created_at": "2026-10-01T00:00:00Z"
        }))
        .unwrap();
        assert!(ArtifactIndexPage::new(vec![artifact.clone(); 101], None).is_err());
        assert!(ArtifactIndexPage::new(vec![], Some(id)).is_err());
        let wire =
            serde_json::to_value(ArtifactIndexPage::new(vec![artifact], None).unwrap()).unwrap();
        assert_eq!(
            wire,
            serde_json::json!({"items": [{
                "uri": metadata_uri(id), "title": "report.txt", "mimeType": "application/json"
            }]})
        );
    }
}
