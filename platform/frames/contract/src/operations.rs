use super::{CoordinateOperationId, FrameOperationUri, FrameUriError, WorldFrameUri};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_map_mcp::contract::CrsId;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
#[serde(rename_all_fields = "camelCase")]
pub enum CoordinateSpace {
    Wgs84,
    EcefWgs84,
    WorldFrame { frame_uri: WorldFrameUri },
}

// Empty wire variants reject undeclared keys while public unit variants stay unchanged.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[serde(rename_all_fields = "camelCase")]
enum CoordinateSpaceWire {
    Wgs84 {},
    EcefWgs84 {},
    WorldFrame { frame_uri: WorldFrameUri },
}

impl<'de> Deserialize<'de> for CoordinateSpace {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match CoordinateSpaceWire::deserialize(deserializer)? {
            CoordinateSpaceWire::Wgs84 {} => Self::Wgs84,
            CoordinateSpaceWire::EcefWgs84 {} => Self::EcefWgs84,
            CoordinateSpaceWire::WorldFrame { frame_uri } => Self::WorldFrame { frame_uri },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateOperationKind {
    FrameConversion,
    CrsTransform,
    LocalFrameDerivation,
    GeodesicInverse,
    GeodesicDirect,
    GeofenceValidation,
    Batch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "OperationRefWire", into = "OperationRefWire")]
pub struct CoordinateOperationRef {
    operation_uri: FrameOperationUri,
    pub source_frame: Option<CoordinateSpace>,
    pub target_frame: Option<CoordinateSpace>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct OperationRefWire {
    operation_id: CoordinateOperationId,
    #[schemars(with = "String")]
    operation_uri: FrameOperationUri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_frame: Option<CoordinateSpace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target_frame: Option<CoordinateSpace>,
    created_at: DateTime<Utc>,
}

impl CoordinateOperationRef {
    pub fn new(operation_id: CoordinateOperationId, created_at: DateTime<Utc>) -> Self {
        Self {
            operation_uri: FrameOperationUri::new(&operation_id),
            source_frame: None,
            target_frame: None,
            created_at,
        }
    }

    pub fn with_frames(
        mut self,
        source: Option<CoordinateSpace>,
        target: Option<CoordinateSpace>,
    ) -> Self {
        self.source_frame = source;
        self.target_frame = target;
        self
    }

    pub fn operation_id(&self) -> &CoordinateOperationId {
        self.operation_uri.operation_id()
    }
    pub fn operation_uri(&self) -> &FrameOperationUri {
        &self.operation_uri
    }
}

impl TryFrom<OperationRefWire> for CoordinateOperationRef {
    type Error = FrameUriError;
    fn try_from(value: OperationRefWire) -> Result<Self, Self::Error> {
        if value.operation_uri.operation_id() != &value.operation_id {
            return Err(FrameUriError::Route);
        }
        Ok(Self {
            operation_uri: value.operation_uri,
            source_frame: value.source_frame,
            target_frame: value.target_frame,
            created_at: value.created_at,
        })
    }
}
impl From<CoordinateOperationRef> for OperationRefWire {
    fn from(value: CoordinateOperationRef) -> Self {
        Self {
            operation_id: value.operation_id().clone(),
            operation_uri: value.operation_uri,
            source_frame: value.source_frame,
            target_frame: value.target_frame,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoordinateOperationProvenance {
    pub operation: CoordinateOperationRef,
    pub kind: CoordinateOperationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_crs: Option<CrsId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_crs: Option<CrsId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grid_packages: Vec<String>,
    #[serde(default)]
    pub approximation_used: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accuracy_m: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operation_provenance_round_trips() {
        let provenance = CoordinateOperationProvenance {
            operation: CoordinateOperationRef::new(
                CoordinateOperationId::parse("op-test").unwrap(),
                "2026-01-01T00:00:00Z".parse().unwrap(),
            )
            .with_frames(
                Some(CoordinateSpace::Wgs84),
                Some(CoordinateSpace::EcefWgs84),
            ),
            kind: CoordinateOperationKind::FrameConversion,
            source_crs: Some(CrsId::parse("EPSG:4326").unwrap()),
            target_crs: Some(CrsId::parse("EPSG:4978").unwrap()),
            engine: Some("test".to_string()),
            grid_packages: Vec::new(),
            approximation_used: false,
            accuracy_m: Some(0.01),
            warnings: Vec::new(),
        };

        let json = serde_json::to_string(&provenance).unwrap();
        let back: CoordinateOperationProvenance = serde_json::from_str(&json).unwrap();
        assert_eq!(
            provenance.operation.operation_id(),
            back.operation.operation_id()
        );
        assert_eq!(provenance.kind, back.kind);
        assert_eq!(provenance.source_crs, back.source_crs);
        assert_eq!(provenance.target_crs, back.target_crs);
    }
}

/// Controlled metadata written beside a Frames batch Artifact.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FramesBatchArtifactMetadata {
    pub task_id: veoveo_types::TaskId,
    pub artifact_format: FramesBatchArtifactFormat,
    pub operation_id: CoordinateOperationId,
    pub operation_uri: FrameOperationUri,
    pub source_frame: Option<CoordinateSpace>,
    pub target_frame: Option<CoordinateSpace>,
    pub source_crs: Option<CrsId>,
    pub target_crs: Option<CrsId>,
    pub approximation_used: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum FramesBatchArtifactFormat {
    #[vocabulary(rename = "frames_batch_json")]
    Json,
}
