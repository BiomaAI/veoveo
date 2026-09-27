use super::{CoordinateOperationId, WorldFrameUri};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_map_mcp::contract::CrsId;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CoordinateSpace {
    Wgs84,
    EcefWgs84,
    WorldFrame { frame_uri: WorldFrameUri },
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
pub struct CoordinateOperationRef {
    pub operation_id: CoordinateOperationId,
    pub operation_uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_frame: Option<CoordinateSpace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_frame: Option<CoordinateSpace>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
            operation: CoordinateOperationRef {
                operation_id: CoordinateOperationId::new("op-test").unwrap(),
                operation_uri: "frames://operation/op-test".to_string(),
                source_frame: Some(CoordinateSpace::Wgs84),
                target_frame: Some(CoordinateSpace::EcefWgs84),
                created_at: "2026-01-01T00:00:00Z".parse().unwrap(),
            },
            kind: CoordinateOperationKind::FrameConversion,
            source_crs: Some(CrsId::new("EPSG:4326").unwrap()),
            target_crs: Some(CrsId::new("EPSG:4978").unwrap()),
            engine: Some("test".to_string()),
            grid_packages: Vec::new(),
            approximation_used: false,
            accuracy_m: Some(0.01),
            warnings: Vec::new(),
        };

        let json = serde_json::to_string(&provenance).unwrap();
        let back: CoordinateOperationProvenance = serde_json::from_str(&json).unwrap();
        assert_eq!(
            provenance.operation.operation_id,
            back.operation.operation_id
        );
        assert_eq!(provenance.kind, back.kind);
        assert_eq!(provenance.source_crs, back.source_crs);
        assert_eq!(provenance.target_crs, back.target_crs);
    }
}
