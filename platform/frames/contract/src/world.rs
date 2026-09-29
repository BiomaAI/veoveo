use super::{FrameEntityPath, FrameId, FrameStreamUri};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
        #[schemars(with = "String")]
        stream_uri: FrameStreamUri,
        #[schemars(with = "String")]
        entity_path: FrameEntityPath,
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
