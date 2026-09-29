mod frame_record;
#[cfg(test)]
pub(crate) mod test_support;
pub use frame_record::{
    CapturedFrame, FrameCaptureBuilder, FrameRecord, FrameRecordError, FrameRenderReport,
};
mod preview_record;
pub use preview_record::{PreviewSceneError, PreviewSceneRecord, SceneTileRecord};
mod view_record;
pub use view_record::{ViewRecord, ViewRecordError};
mod resources;
pub use resources::*;
mod scopes;
pub use scopes::ViewScope;
mod task_kind;
use std::{fmt, str::FromStr};
pub use task_kind::ViewTaskKind;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod composition;

pub use composition::*;

fn validate_id(value: &str) -> Result<(), ContractError> {
    if value.is_empty()
        || matches!(value, "." | "..")
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ContractError::InvalidIdentifier(value.to_owned()));
    }
    Ok(())
}

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ContractError> {
                let value = value.into();
                validate_id(&value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = ContractError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = ContractError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

id_type!(ViewId);
id_type!(FrameId);
id_type!(LayerId);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Wgs84Position3d {
    pub latitude_degrees: f64,
    pub longitude_degrees: f64,
    pub ellipsoidal_height_meters: f64,
}

impl Wgs84Position3d {
    pub fn validate(self) -> Result<Self, ContractError> {
        if !self.latitude_degrees.is_finite() || !(-90.0..=90.0).contains(&self.latitude_degrees) {
            return Err(ContractError::InvalidLatitude);
        }
        if !self.longitude_degrees.is_finite()
            || !(-180.0..=180.0).contains(&self.longitude_degrees)
        {
            return Err(ContractError::InvalidLongitude);
        }
        if !self.ellipsoidal_height_meters.is_finite()
            || !(-20_000.0..=100_000_000.0).contains(&self.ellipsoidal_height_meters)
        {
            return Err(ContractError::InvalidHeight);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HeadingPitchRoll {
    pub heading_degrees: f64,
    pub pitch_degrees: f64,
    pub roll_degrees: f64,
}

impl HeadingPitchRoll {
    pub fn validate(self) -> Result<Self, ContractError> {
        if !self.heading_degrees.is_finite()
            || !self.pitch_degrees.is_finite()
            || !self.roll_degrees.is_finite()
            || !(-90.0..=90.0).contains(&self.pitch_degrees)
        {
            return Err(ContractError::InvalidOrientation);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeodeticCameraPose {
    pub position: Wgs84Position3d,
    pub orientation: HeadingPitchRoll,
    pub vertical_fov_degrees: f32,
}

impl GeodeticCameraPose {
    pub fn validate(self) -> Result<Self, ContractError> {
        self.position.validate()?;
        self.orientation.validate()?;
        validate_fov(self.vertical_fov_degrees)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LookAtCamera {
    pub eye: Wgs84Position3d,
    pub target: Wgs84Position3d,
    pub vertical_fov_degrees: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OrbitTargetCamera {
    pub target: Wgs84Position3d,
    pub distance_meters: f64,
    pub azimuth_degrees: f64,
    pub elevation_degrees: f64,
    pub vertical_fov_degrees: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CameraDefinition {
    Pose(GeodeticCameraPose),
    LookAt(LookAtCamera),
    OrbitTarget(OrbitTargetCamera),
}

impl CameraDefinition {
    pub fn validate(self) -> Result<Self, ContractError> {
        match &self {
            Self::Pose(pose) => {
                pose.clone().validate()?;
            }
            Self::LookAt(camera) => {
                camera.eye.validate()?;
                camera.target.validate()?;
                validate_fov(camera.vertical_fov_degrees)?;
                if camera.eye == camera.target {
                    return Err(ContractError::CoincidentEyeAndTarget);
                }
            }
            Self::OrbitTarget(camera) => {
                camera.target.validate()?;
                validate_fov(camera.vertical_fov_degrees)?;
                if !camera.distance_meters.is_finite()
                    || !(0.1..=100_000_000.0).contains(&camera.distance_meters)
                    || !camera.azimuth_degrees.is_finite()
                    || !camera.elevation_degrees.is_finite()
                    || !(-89.9..=89.9).contains(&camera.elevation_degrees)
                {
                    return Err(ContractError::InvalidOrbit);
                }
            }
        }
        Ok(self)
    }
}

fn validate_fov(value: f32) -> Result<(), ContractError> {
    if value.is_finite() && (1.0..=160.0).contains(&value) {
        Ok(())
    } else {
        Err(ContractError::InvalidFieldOfView)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeadlineBehavior {
    ReturnBestAvailable,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FrameEncoding {
    Png,
    Jpeg,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CapturePolicy {
    pub width_px: u32,
    pub height_px: u32,
    pub max_screen_error_px: f32,
    pub deadline_ms: u32,
    #[serde(default = "default_deadline_behavior")]
    pub deadline_behavior: DeadlineBehavior,
    #[serde(default = "default_frame_encoding")]
    pub encoding: FrameEncoding,
}

impl FrameEncoding {
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }
}

// Record admission checks the representable contract shape; runtime configuration
// supplies the installation's stricter limits before work is dispatched.
const RECORD_CAPTURE_LIMITS: CaptureLimits = CaptureLimits {
    max_width_px: u32::MAX,
    max_height_px: u32::MAX,
    max_pixels: u64::MAX,
    max_deadline_ms: u32::MAX,
};

fn default_deadline_behavior() -> DeadlineBehavior {
    DeadlineBehavior::ReturnBestAvailable
}

fn default_frame_encoding() -> FrameEncoding {
    FrameEncoding::Jpeg
}

impl CapturePolicy {
    pub fn validate(&self, limits: &CaptureLimits) -> Result<(), ContractError> {
        validate_render_policy(
            self.width_px,
            self.height_px,
            self.max_screen_error_px,
            limits,
        )?;
        if self.deadline_ms == 0 || self.deadline_ms > limits.max_deadline_ms {
            return Err(ContractError::InvalidDeadline {
                max_deadline_ms: limits.max_deadline_ms,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PreviewScenePolicy {
    pub width_px: u32,
    pub height_px: u32,
    pub max_screen_error_px: f32,
}

impl PreviewScenePolicy {
    pub fn validate(&self, limits: &CaptureLimits) -> Result<(), ContractError> {
        validate_render_policy(
            self.width_px,
            self.height_px,
            self.max_screen_error_px,
            limits,
        )
    }
}

fn validate_render_policy(
    width_px: u32,
    height_px: u32,
    max_screen_error_px: f32,
    limits: &CaptureLimits,
) -> Result<(), ContractError> {
    if width_px == 0
        || height_px == 0
        || width_px > limits.max_width_px
        || height_px > limits.max_height_px
        || u64::from(width_px) * u64::from(height_px) > limits.max_pixels
    {
        return Err(ContractError::InvalidViewport {
            max_width_px: limits.max_width_px,
            max_height_px: limits.max_height_px,
            max_pixels: limits.max_pixels,
        });
    }
    if !max_screen_error_px.is_finite() || !(0.25..=256.0).contains(&max_screen_error_px) {
        return Err(ContractError::InvalidScreenError);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct CaptureLimits {
    pub max_width_px: u32,
    pub max_height_px: u32,
    pub max_pixels: u64,
    pub max_deadline_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AttributionSet {
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateViewRequest {
    pub composition_id: SceneCompositionId,
    pub camera: CameraDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SetCameraRequest {
    pub view_id: ViewId,
    pub expected_revision: u64,
    pub camera: CameraDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CaptureFrameRequest {
    pub view_id: ViewId,
    pub expected_revision: u64,
    pub scene_time: DateTime<Utc>,
    pub policy: CapturePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CloseViewRequest {
    pub view_id: ViewId,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CloseViewResult {
    pub view_id: ViewId,
    pub closed: bool,
}

/// A preview carries the complete render cut requested by its typed scene
/// policy unless this transport guard is reached.
pub const SCENE_MAX_TILES: usize = 256;
pub const SCENE_DEADLINE_MS: u64 = 30_000;
/// Raw tile ceiling: base64(1.5 MB) plus the JSON envelope stays under the
/// console host's 2 MiB resource-read cap.
pub const MAX_TILE_RESOURCE_BYTES: u64 = 1_500_000;

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("identifier `{0}` is invalid; use 1 to 128 ASCII letters, digits, `.`, `_`, or `-`")]
    InvalidIdentifier(String),
    #[error("latitude must be finite and between -90 and 90 degrees")]
    InvalidLatitude,
    #[error("longitude must be finite and between -180 and 180 degrees")]
    InvalidLongitude,
    #[error("ellipsoidal height must be finite and between -20000 and 100000000 meters")]
    InvalidHeight,
    #[error("heading, pitch, and roll must be finite and pitch must be between -90 and 90")]
    InvalidOrientation,
    #[error("vertical field of view must be between 1 and 160 degrees")]
    InvalidFieldOfView,
    #[error("camera eye and target must differ")]
    CoincidentEyeAndTarget,
    #[error(
        "orbit camera needs a finite azimuth, a distance between 0.1 and 100000000 meters, and an elevation between -89.9 and 89.9 degrees"
    )]
    InvalidOrbit,
    #[error(
        "viewport must be at least 1x1 and at most {max_width_px}x{max_height_px} pixels, with no more than {max_pixels} pixels in total"
    )]
    InvalidViewport {
        max_width_px: u32,
        max_height_px: u32,
        max_pixels: u64,
    },
    #[error("maximum screen error must be between 0.25 and 256 pixels")]
    InvalidScreenError,
    #[error("capture deadline must be between 1 and {max_deadline_ms} milliseconds")]
    InvalidDeadline { max_deadline_ms: u32 },
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn orbit_camera() -> Value {
        json!({
            "kind": "orbit_target",
            "target": {
                "latitude_degrees": 37.8199,
                "longitude_degrees": -122.4783,
                "ellipsoidal_height_meters": 80.0
            },
            "distance_meters": 1_200.0,
            "azimuth_degrees": 135.0,
            "elevation_degrees": 35.0,
            "vertical_fov_degrees": 55.0
        })
    }

    fn capture_policy() -> Value {
        json!({
            "width_px": 1_280,
            "height_px": 720,
            "max_screen_error_px": 4.0,
            "deadline_ms": 30_000,
            "deadline_behavior": "return_best_available",
            "encoding": "jpeg"
        })
    }

    fn input_schemas<T: JsonSchema + 'static>() -> Vec<Value> {
        let schema = serde_json::to_value(schemars::schema_for!(T)).unwrap();
        #[cfg(feature = "mcp")]
        let schemas = {
            let mcp = rmcp::handler::server::tool::schema_for_type::<T>();
            vec![schema, Value::Object(mcp.as_ref().clone())]
        };
        #[cfg(not(feature = "mcp"))]
        let schemas = vec![schema];
        for schema in &schemas {
            jsonschema::meta::validate(schema)
                .unwrap_or_else(|error| panic!("invalid generated JSON Schema: {error}"));
        }
        schemas
    }

    #[test]
    fn ids_are_bounded_and_uri_safe() {
        assert!(LayerId::new("google-photorealistic").is_ok());
        assert!(LayerId::new("bad/id").is_err());
        assert!(LayerId::new("").is_err());
    }

    #[test]
    fn pose_validation_rejects_invalid_latitude() {
        let pose = GeodeticCameraPose {
            position: Wgs84Position3d {
                latitude_degrees: 91.0,
                longitude_degrees: 0.0,
                ellipsoidal_height_meters: 10.0,
            },
            orientation: HeadingPitchRoll {
                heading_degrees: 0.0,
                pitch_degrees: 0.0,
                roll_degrees: 0.0,
            },
            vertical_fov_degrees: 45.0,
        };
        assert!(pose.validate().is_err());
    }

    #[test]
    fn structured_arguments_validate_against_enabled_schema_profiles() {
        let create = json!({
            "composition_id": SceneCompositionId::from_stable_key(b"schema-test"),
            "camera": orbit_camera()
        });
        for schema in input_schemas::<CreateViewRequest>() {
            assert!(jsonschema::is_valid(&schema, &create));
        }

        let capture = json!({
            "view_id": "view-1",
            "expected_revision": 1,
            "scene_time": "2026-07-26T12:00:00Z",
            "policy": capture_policy()
        });
        for schema in input_schemas::<CaptureFrameRequest>() {
            assert!(jsonschema::is_valid(&schema, &capture));
        }
    }
}
