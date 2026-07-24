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
coordinate_id!(
    FrameWorldId,
    "Stable identity of one authored coordinate-frame world."
);
coordinate_id!(
    FrameWorldRevisionId,
    "Immutable identity of one complete coordinate-frame world revision."
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Wgs84Position {
    #[schemars(range(min = -90.0, max = 90.0))]
    pub latitude_degrees: f64,
    #[schemars(range(min = -180.0, max = 180.0))]
    pub longitude_degrees: f64,
    pub ellipsoid_height_m: f64,
}

impl Wgs84Position {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.latitude_degrees.is_finite()
            || !self.longitude_degrees.is_finite()
            || !self.ellipsoid_height_m.is_finite()
        {
            return Err("WGS84 coordinates must be finite");
        }
        if !(-90.0..=90.0).contains(&self.latitude_degrees) {
            return Err("latitude_degrees must be within [-90, 90]");
        }
        if !(-180.0..=180.0).contains(&self.longitude_degrees) {
            return Err("longitude_degrees must be within [-180, 180]");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FrameAxisDirection {
    Right,
    Left,
    Up,
    Down,
    Forward,
    Back,
}

impl FrameAxisDirection {
    pub const fn unsigned_axis(self) -> u8 {
        match self {
            Self::Right | Self::Left => 0,
            Self::Up | Self::Down => 1,
            Self::Forward | Self::Back => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameAxes {
    pub x: FrameAxisDirection,
    pub y: FrameAxisDirection,
    pub z: FrameAxisDirection,
}

impl FrameAxes {
    pub const fn east_north_up() -> Self {
        Self {
            x: FrameAxisDirection::Right,
            y: FrameAxisDirection::Forward,
            z: FrameAxisDirection::Up,
        }
    }

    pub const fn north_east_down() -> Self {
        Self {
            x: FrameAxisDirection::Forward,
            y: FrameAxisDirection::Right,
            z: FrameAxisDirection::Down,
        }
    }

    pub const fn forward_right_down() -> Self {
        Self::north_east_down()
    }

    pub const fn right_down_forward() -> Self {
        Self {
            x: FrameAxisDirection::Right,
            y: FrameAxisDirection::Down,
            z: FrameAxisDirection::Forward,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        let mut axes = [
            self.x.unsigned_axis(),
            self.y.unsigned_axis(),
            self.z.unsigned_axis(),
        ];
        axes.sort_unstable();
        if axes == [0, 1, 2] {
            Ok(())
        } else {
            Err("frame axes must cover each unsigned cardinal axis exactly once")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FrameBasis {
    EcefWgs84,
    Enu,
    Ned,
    Frd,
    OpticalRdf,
    Cartesian { axes: FrameAxes },
}

impl FrameBasis {
    pub fn axes(&self) -> FrameAxes {
        match self {
            Self::EcefWgs84 => FrameAxes {
                x: FrameAxisDirection::Right,
                y: FrameAxisDirection::Up,
                z: FrameAxisDirection::Back,
            },
            Self::Enu => FrameAxes::east_north_up(),
            Self::Ned | Self::Frd => FrameAxes::forward_right_down(),
            Self::OpticalRdf => FrameAxes::right_down_forward(),
            Self::Cartesian { axes } => axes.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FrameParentTransform {
    GeodeticTangent {
        origin: Wgs84Position,
    },
    StaticRigid {
        translation_m: [f64; 3],
        rotation_xyzw: [f64; 4],
    },
    DynamicStream {
        stream_uri: String,
        entity_path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameNode {
    pub frame_id: FrameId,
    pub basis: FrameBasis,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_frame_id: Option<FrameId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_transform: Option<FrameParentTransform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameWorldTree {
    #[schemars(length(min = 1, max = 10_000))]
    pub frames: Vec<FrameNode>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct FrameWorldUri(String);

impl FrameWorldUri {
    pub fn new(world_id: &FrameWorldId) -> Self {
        Self(format!("frames://world/{world_id}"))
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, CoordinateIdError> {
        let value = value.into();
        let Some(world_id) = value.strip_prefix("frames://world/") else {
            return Err(CoordinateIdError::new(
                &value,
                "must use frames://world/{world_id}",
            ));
        };
        if world_id.contains('/') {
            return Err(CoordinateIdError::new(
                &value,
                "must identify exactly one frame world",
            ));
        }
        FrameWorldId::new(world_id)?;
        Ok(Self(value))
    }

    pub fn world_id(&self) -> FrameWorldId {
        let world_id = self
            .0
            .strip_prefix("frames://world/")
            .expect("validated frame world URI");
        FrameWorldId::new(world_id).expect("validated frame world id")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for FrameWorldUri {
    type Error = CoordinateIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<FrameWorldUri> for String {
    fn from(value: FrameWorldUri) -> Self {
        value.0
    }
}

impl fmt::Display for FrameWorldUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct FrameWorldRevisionUri(String);

impl FrameWorldRevisionUri {
    pub fn new(world_id: &FrameWorldId, revision_id: &FrameWorldRevisionId) -> Self {
        Self(format!("frames://world/{world_id}/revision/{revision_id}"))
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, CoordinateIdError> {
        let value = value.into();
        let Some(rest) = value.strip_prefix("frames://world/") else {
            return Err(CoordinateIdError::new(
                &value,
                "must use frames://world/{world_id}/revision/{revision_id}",
            ));
        };
        let Some((world_id, revision_id)) = rest.split_once("/revision/") else {
            return Err(CoordinateIdError::new(
                &value,
                "must use frames://world/{world_id}/revision/{revision_id}",
            ));
        };
        if revision_id.contains('/') {
            return Err(CoordinateIdError::new(
                &value,
                "must identify exactly one world revision",
            ));
        }
        FrameWorldId::new(world_id)?;
        FrameWorldRevisionId::new(revision_id)?;
        Ok(Self(value))
    }

    pub fn world_id(&self) -> FrameWorldId {
        let rest = self
            .0
            .strip_prefix("frames://world/")
            .expect("validated world revision URI");
        let (world_id, _) = rest
            .split_once("/revision/")
            .expect("validated world revision URI");
        FrameWorldId::new(world_id).expect("validated world id")
    }

    pub fn revision_id(&self) -> FrameWorldRevisionId {
        let revision_id = self
            .0
            .rsplit_once("/revision/")
            .expect("validated world revision URI")
            .1;
        FrameWorldRevisionId::new(revision_id).expect("validated world revision id")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for FrameWorldRevisionUri {
    type Error = CoordinateIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<FrameWorldRevisionUri> for String {
    fn from(value: FrameWorldRevisionUri) -> Self {
        value.0
    }
}

impl fmt::Display for FrameWorldRevisionUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct WorldFrameUri(String);

impl WorldFrameUri {
    pub fn new(revision: &FrameWorldRevisionUri, frame_id: &FrameId) -> Self {
        Self(format!("{revision}/frame/{frame_id}"))
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, CoordinateIdError> {
        let value = value.into();
        let Some((revision, frame_id)) = value.rsplit_once("/frame/") else {
            return Err(CoordinateIdError::new(
                &value,
                "must identify a frame inside an immutable world revision",
            ));
        };
        FrameWorldRevisionUri::parse(revision.to_owned())?;
        FrameId::new(frame_id)?;
        Ok(Self(value))
    }

    pub fn revision_uri(&self) -> FrameWorldRevisionUri {
        let revision = self
            .0
            .rsplit_once("/frame/")
            .expect("validated world frame URI")
            .0;
        FrameWorldRevisionUri::parse(revision.to_owned()).expect("validated revision URI")
    }

    pub fn frame_id(&self) -> FrameId {
        let frame_id = self
            .0
            .rsplit_once("/frame/")
            .expect("validated world frame URI")
            .1;
        FrameId::new(frame_id).expect("validated frame id")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for WorldFrameUri {
    type Error = CoordinateIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<WorldFrameUri> for String {
    fn from(value: WorldFrameUri) -> Self {
        value.0
    }
}

impl fmt::Display for WorldFrameUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameWorldRevision {
    pub world_id: FrameWorldId,
    pub world_uri: FrameWorldUri,
    pub revision_id: FrameWorldRevisionId,
    pub revision_uri: FrameWorldRevisionUri,
    pub revision: u64,
    pub spec_sha256: String,
    pub root_frame_uri: WorldFrameUri,
    pub tree: FrameWorldTree,
    pub created_at: DateTime<Utc>,
}

impl FrameWorldRevision {
    pub fn frame(&self, frame_uri: &WorldFrameUri) -> Option<&FrameNode> {
        if frame_uri.revision_uri() != self.revision_uri {
            return None;
        }
        let frame_id = frame_uri.frame_id();
        self.tree
            .frames
            .iter()
            .find(|frame| frame.frame_id == frame_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CoordinateSpace {
    Wgs84,
    EcefWgs84,
    WorldFrame { frame_uri: WorldFrameUri },
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
                operation_uri: "frames://operation/op-test".to_string(),
                source_frame: Some(CoordinateSpace::Wgs84),
                target_frame: Some(CoordinateSpace::EcefWgs84),
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
