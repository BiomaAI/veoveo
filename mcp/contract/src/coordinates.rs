use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn validate_coordinate_id(value: &str) -> Result<(), CoordinateIdError> {
    if value.is_empty() || value.len() > 128 {
        return Err(CoordinateIdError::new(value, "must be 1 to 128 characters"));
    }
    if value.contains("://") || value.contains('/') || value.chars().any(char::is_whitespace) {
        return Err(CoordinateIdError::new(
            value,
            "must not contain whitespace, slash, or URI separators",
        ));
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':'))
    {
        Ok(())
    } else {
        Err(CoordinateIdError::new(
            value,
            "must contain only ASCII letters, digits, underscore, dash, dot, or colon",
        ))
    }
}

macro_rules! coordinate_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, CoordinateIdError> {
                let value = value.into();
                validate_coordinate_id(&value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = CoordinateIdError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl FromStr for $name {
            type Err = CoordinateIdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value.to_string())
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateIdError {
    value: String,
    rule: &'static str,
}

impl CoordinateIdError {
    fn new(value: &str, rule: &'static str) -> Self {
        Self {
            value: value.to_string(),
            rule,
        }
    }
}

impl fmt::Display for CoordinateIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid coordinate identifier {:?}: {}",
            self.value, self.rule
        )
    }
}

impl std::error::Error for CoordinateIdError {}

coordinate_id!(
    FrameId,
    "Coordinate operation frame id for solver inputs, resources, and provenance. RRD transform frame ids live in veoveo-rrd."
);
coordinate_id!(CrsId, "Coordinate reference system id, commonly EPSG:4326.");
coordinate_id!(
    DatumId,
    "Geodetic datum id used by a CRS or coordinate operation."
);
coordinate_id!(
    EllipsoidId,
    "Reference ellipsoid id, such as WGS84 or GRS80."
);
coordinate_id!(
    CoordinateOperationId,
    "Durable id for one coordinate transform, projection, geodesic, or validation operation."
);
coordinate_id!(
    TrajectoryId,
    "Trajectory identity used by plan and robot outputs."
);
coordinate_id!(
    GeofenceId,
    "Geofence identity used by validation and plans."
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FrameKind {
    Wgs84,
    Ecef,
    Enu,
    Ned,
    Frd,
    ProjectedCrs,
    SimulationWorld,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GeofenceRule {
    MustStayInside,
    MustStayOutside,
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
    pub source_frame: Option<FrameId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_frame: Option<FrameId>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeofenceViolation {
    pub index: usize,
    pub point: [f64; 2],
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_ids_accept_crs_and_frame_forms() {
        assert_eq!(CrsId::new("EPSG:4326").unwrap().as_str(), "EPSG:4326");
        assert_eq!(
            FrameId::new("ENU:mission-alpha").unwrap().as_str(),
            "ENU:mission-alpha"
        );
        assert!(FrameId::new("bad/id").is_err());
        assert!(FrameId::new("bad id").is_err());
    }

    #[test]
    fn operation_provenance_round_trips() {
        let provenance = CoordinateOperationProvenance {
            operation: CoordinateOperationRef {
                operation_id: CoordinateOperationId::new("op-test").unwrap(),
                operation_uri: "coordinates://operation/op-test".to_string(),
                source_frame: Some(FrameId::new("WGS84").unwrap()),
                target_frame: Some(FrameId::new("ECEF").unwrap()),
                created_at: Utc::now(),
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
