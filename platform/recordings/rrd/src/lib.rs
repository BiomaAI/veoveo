mod spatial_metadata;
pub use spatial_metadata::{FrameKind, GeofenceId, GeofenceIdError, GeofenceRule};

use std::{collections::BTreeMap, fmt};

use re_sdk_types::{
    archetypes::{CoordinateFrame, GeoLineStrings, GeoPoints, LineStrips3D, Points3D, Transform3D},
    components::{GeoLineString, LatLon, LineStrip3D, TransformFrameId, ViewCoordinates},
    view_coordinates::ViewDir,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_map_mcp::contract::{CrsId, DatumId, EllipsoidId};

pub mod ingest_parts;
pub mod projection;
pub mod properties_layer;
pub mod recording_layer;
pub mod segment;
pub mod video;
pub mod video_clip;

fn validate_rrd_id(value: &str, kind: &'static str) -> Result<(), RrdIdError> {
    if value.is_empty() || value.len() > 512 {
        return Err(RrdIdError::new(value, kind, "must be 1 to 512 characters"));
    }
    if value.chars().any(char::is_whitespace) {
        return Err(RrdIdError::new(value, kind, "must not contain whitespace"));
    }
    if value.contains("://") {
        return Err(RrdIdError::new(value, kind, "must not be a URI"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RrdIdError {
    value: String,
    kind: &'static str,
    rule: &'static str,
}

impl RrdIdError {
    fn new(value: &str, kind: &'static str, rule: &'static str) -> Self {
        Self {
            value: value.to_string(),
            kind,
            rule,
        }
    }
}

impl fmt::Display for RrdIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {} {:?}: {}", self.kind, self.value, self.rule)
    }
}

impl std::error::Error for RrdIdError {}

#[doc = "Rerun transform frame id. Entity-derived ids use Rerun's `tf#/path` convention."]
#[veoveo_types::id(text(RrdIds), error_context = "RRD transform frame id")]
pub struct RrdFrameId(String);
#[doc = "Rerun entity path used to locate canonical spacetime data in an RRD recording."]
#[veoveo_types::id(text(RrdIds), error_context = "RRD entity path")]
pub struct RrdEntityPath(String);
#[doc = "Rerun timeline name used to scrub or query state over time."]
#[veoveo_types::id(text(RrdIds), error_context = "RRD timeline")]
pub struct RrdTimeline(String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RrdTimePoint {
    pub timeline: RrdTimeline,
    pub sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RrdTimeRange {
    pub timeline: RrdTimeline,
    pub start_sequence: i64,
    pub end_sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RrdRecordingRef {
    Live { recording_id: String },
    Artifact { uri: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RrdSelection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording: Option<RrdRecordingRef>,
    pub entity_path: RrdEntityPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_range: Option<RrdTimeRange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RrdGeoPoint {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_m: Option<f64>,
}

impl RrdGeoPoint {
    pub fn validate(&self) -> Result<(), RrdValueError> {
        if !self.latitude_deg.is_finite()
            || !self.longitude_deg.is_finite()
            || !self.height_m.unwrap_or_default().is_finite()
        {
            return Err(RrdValueError::new("coordinates must be finite"));
        }
        if !(-90.0..=90.0).contains(&self.latitude_deg) {
            return Err(RrdValueError::new("latitude_deg must be within [-90, 90]"));
        }
        if !(-180.0..=180.0).contains(&self.longitude_deg) {
            return Err(RrdValueError::new(
                "longitude_deg must be within [-180, 180]",
            ));
        }
        Ok(())
    }

    pub fn to_rerun_lat_lon(&self) -> LatLon {
        LatLon::new(self.latitude_deg, self.longitude_deg)
    }
}

impl From<RrdGeoPoint> for LatLon {
    fn from(value: RrdGeoPoint) -> Self {
        value.to_rerun_lat_lon()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RrdLocalPoint3 {
    pub frame_id: RrdFrameId,
    pub xyz_m: [f64; 3],
}

impl RrdLocalPoint3 {
    pub fn to_rerun_point(&self) -> [f32; 3] {
        [
            self.xyz_m[0] as f32,
            self.xyz_m[1] as f32,
            self.xyz_m[2] as f32,
        ]
    }

    pub fn to_rerun_transform3d(&self) -> Transform3D {
        Transform3D::new()
            .with_child_frame(self.frame_id.as_str())
            .with_translation(self.to_rerun_point())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RrdViewDirection {
    Right,
    Left,
    Up,
    Down,
    Forward,
    Back,
}

impl From<RrdViewDirection> for ViewDir {
    fn from(value: RrdViewDirection) -> Self {
        match value {
            RrdViewDirection::Right => ViewDir::Right,
            RrdViewDirection::Left => ViewDir::Left,
            RrdViewDirection::Up => ViewDir::Up,
            RrdViewDirection::Down => ViewDir::Down,
            RrdViewDirection::Forward => ViewDir::Forward,
            RrdViewDirection::Back => ViewDir::Back,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RrdViewCoordinates {
    pub x: RrdViewDirection,
    pub y: RrdViewDirection,
    pub z: RrdViewDirection,
}

impl RrdViewCoordinates {
    pub const fn new(x: RrdViewDirection, y: RrdViewDirection, z: RrdViewDirection) -> Self {
        Self { x, y, z }
    }

    pub const fn east_north_up() -> Self {
        Self::new(
            RrdViewDirection::Right,
            RrdViewDirection::Forward,
            RrdViewDirection::Up,
        )
    }

    pub const fn north_east_down() -> Self {
        Self::new(
            RrdViewDirection::Forward,
            RrdViewDirection::Right,
            RrdViewDirection::Down,
        )
    }

    pub const fn forward_right_down() -> Self {
        Self::north_east_down()
    }

    pub const fn xyz_meters() -> Self {
        Self::new(
            RrdViewDirection::Right,
            RrdViewDirection::Up,
            RrdViewDirection::Back,
        )
    }

    pub fn to_rerun_view_coordinates(&self) -> Result<ViewCoordinates, RrdValueError> {
        let view = ViewCoordinates::new(self.x.into(), self.y.into(), self.z.into());
        view.sanity_check().map_err(|_| {
            RrdValueError::new("view coordinates must cover three cardinal axes exactly once")
        })?;
        Ok(view)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RrdFrameDefinition {
    pub frame_id: RrdFrameId,
    pub kind: FrameKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_coordinates: Option<RrdViewCoordinates>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<RrdFrameId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<RrdGeoPoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crs: Option<CrsId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datum: Option<DatumId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ellipsoid: Option<EllipsoidId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epoch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl RrdFrameDefinition {
    pub fn coordinate_frame(&self) -> CoordinateFrame {
        CoordinateFrame::new(self.frame_id.as_str())
    }

    pub fn static_transform(&self) -> Transform3D {
        let transform = Transform3D::new().with_child_frame(self.frame_id.as_str());
        if let Some(parent) = &self.parent {
            transform.with_parent_frame(parent.as_str())
        } else {
            transform
        }
    }

    pub fn origin_geo_points(&self) -> Option<GeoPoints> {
        self.origin
            .as_ref()
            .map(|origin| GeoPoints::from_lat_lon([origin.to_rerun_lat_lon()]))
    }
}

impl TryFrom<&RrdFrameDefinition> for CoordinateFrame {
    type Error = RrdValueError;

    fn try_from(value: &RrdFrameDefinition) -> Result<Self, Self::Error> {
        Ok(value.coordinate_frame())
    }
}

impl From<RrdFrameId> for TransformFrameId {
    fn from(value: RrdFrameId) -> Self {
        TransformFrameId::new(value.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RrdLocalLineString2 {
    pub frame_id: RrdFrameId,
    pub coordinates: Vec<[f64; 2]>,
}

impl RrdLocalLineString2 {
    pub fn to_rerun_line_strips3d(&self) -> LineStrips3D {
        let strip = LineStrip3D::from_iter(
            self.coordinates
                .iter()
                .map(|point| [point[0] as f32, point[1] as f32, 0.0]),
        );
        LineStrips3D::new([strip])
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RrdGeoLineString {
    pub coordinates: Vec<[f64; 2]>,
}

impl RrdGeoLineString {
    pub fn to_rerun_geo_line_strings(&self) -> GeoLineStrings {
        let line =
            GeoLineString::from_iter(self.coordinates.iter().map(|point| [point[0], point[1]]));
        GeoLineStrings::from_lat_lon([line])
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RrdLocalPolygon2 {
    pub frame_id: RrdFrameId,
    pub exterior: Vec<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Vec<[f64; 2]>>,
}

impl RrdLocalPolygon2 {
    pub fn exterior_line_string(&self) -> RrdLocalLineString2 {
        RrdLocalLineString2 {
            frame_id: self.frame_id.clone(),
            coordinates: self.exterior.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RrdGeofenceGeometry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geofence_id: Option<GeofenceId>,
    pub rule: GeofenceRule,
    pub polygon: RrdLocalPolygon2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RrdValueError {
    rule: &'static str,
}

impl RrdValueError {
    pub fn new(rule: &'static str) -> Self {
        Self { rule }
    }
}

impl fmt::Display for RrdValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.rule)
    }
}

impl std::error::Error for RrdValueError {}

pub fn geo_points(points: &[RrdGeoPoint]) -> GeoPoints {
    GeoPoints::from_lat_lon(points.iter().map(RrdGeoPoint::to_rerun_lat_lon))
}

pub fn local_points3d(points: &[RrdLocalPoint3]) -> Points3D {
    Points3D::new(points.iter().map(RrdLocalPoint3::to_rerun_point))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_ids_accept_rerun_entity_frames() {
        let frame = RrdFrameId::parse("tf#/mission/agent-a").unwrap();
        assert_eq!(frame.as_str(), "tf#/mission/agent-a");
        assert!(RrdFrameId::parse("bad frame").is_err());
    }

    #[test]
    fn geo_point_json_round_trip_preserves_height() {
        let point = RrdGeoPoint {
            latitude_deg: 59.319_221,
            longitude_deg: 18.075_631,
            height_m: Some(42.0),
        };
        point.validate().unwrap();
        let json = serde_json::to_string(&point).unwrap();
        let back: RrdGeoPoint = serde_json::from_str(&json).unwrap();
        assert_eq!(point, back);
    }

    #[test]
    fn selection_json_round_trip_preserves_recording_and_time_range() {
        let selection = RrdSelection {
            recording: Some(RrdRecordingRef::Artifact {
                uri: "artifact://rrd/abc123".to_string(),
            }),
            entity_path: RrdEntityPath::parse("/mission/agent-a").unwrap(),
            time_range: Some(RrdTimeRange {
                timeline: RrdTimeline::parse("tick").unwrap(),
                start_sequence: 10,
                end_sequence: 20,
            }),
        };
        let json = serde_json::to_string(&selection).unwrap();
        let back: RrdSelection = serde_json::from_str(&json).unwrap();
        assert_eq!(selection, back);
    }

    #[test]
    fn view_coordinates_convert_to_rerun() {
        let view = RrdViewCoordinates::east_north_up()
            .to_rerun_view_coordinates()
            .unwrap();
        assert_eq!(view.describe_short(), "RFU");
    }

    #[test]
    fn frame_definition_round_trip_and_adapters_construct() {
        let frame = RrdFrameDefinition {
            frame_id: RrdFrameId::parse("ENU:mission").unwrap(),
            kind: FrameKind::Enu,
            view_coordinates: Some(RrdViewCoordinates::east_north_up()),
            parent: Some(RrdFrameId::parse("WGS84").unwrap()),
            origin: Some(RrdGeoPoint {
                latitude_deg: 37.0,
                longitude_deg: -122.0,
                height_m: Some(10.0),
            }),
            crs: Some(CrsId::parse("EPSG:4326").unwrap()),
            datum: Some(DatumId::parse("WGS84").unwrap()),
            ellipsoid: Some(EllipsoidId::parse("WGS84").unwrap()),
            epoch: None,
            description: Some("mission local frame".to_string()),
            metadata: BTreeMap::new(),
        };
        let json = serde_json::to_string(&frame).unwrap();
        let back: RrdFrameDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(frame, back);
        assert!(back.coordinate_frame().frame.is_some());
        assert!(back.static_transform().child_frame.is_some());
        assert!(back.origin_geo_points().unwrap().positions.is_some());
    }

    #[test]
    fn local_geometry_round_trip_and_rerun_adapter_construct() {
        let geofence = RrdGeofenceGeometry {
            geofence_id: None,
            rule: GeofenceRule::MustStayInside,
            polygon: RrdLocalPolygon2 {
                frame_id: RrdFrameId::parse("ENU:test").unwrap(),
                exterior: vec![
                    [0.0, 0.0],
                    [10.0, 0.0],
                    [10.0, 10.0],
                    [0.0, 10.0],
                    [0.0, 0.0],
                ],
                holes: Vec::new(),
            },
        };
        let json = serde_json::to_string(&geofence).unwrap();
        let back: RrdGeofenceGeometry = serde_json::from_str(&json).unwrap();
        assert_eq!(geofence, back);
        assert!(
            back.polygon
                .exterior_line_string()
                .to_rerun_line_strips3d()
                .strips
                .is_some()
        );
        assert!(
            RrdGeoLineString {
                coordinates: vec![[59.319_221, 18.075_631], [59.320, 18.076]],
            }
            .to_rerun_geo_line_strings()
            .line_strings
            .is_some()
        );
    }
}

#[cfg(test)]
mod identity_profiles {
    use super::*;
    use veoveo_types::Identity;
    #[test]
    fn rrd_names_keep_their_own_spelling_and_length_profile() {
        let frame = RrdFrameId::parse_identity("tf#/world/源").unwrap();
        assert_eq!(frame.identity_text(), "tf#/world/源");
        assert_eq!(serde_json::to_value(frame).unwrap(), "tf#/world/源");
        assert!(RrdEntityPath::parse_identity("/sensor/camera").is_ok());
        assert!(RrdTimeline::parse_identity(&"t".repeat(512)).is_ok());
        assert!(RrdTimeline::parse_identity(&"t".repeat(513)).is_err());
        for value in ["", "white space", "recording://resource"] {
            assert!(RrdFrameId::parse_identity(value).is_err());
        }
        assert!(GeofenceId::parse_identity("zone:1").is_ok());
        assert!(GeofenceId::parse_identity("zone/1").is_err());
        assert!(serde_json::from_value::<GeofenceId>(serde_json::json!("zone/1")).is_err());
    }
}

use veoveo_types::{IdProfile, IdProfileSpec};

#[doc(hidden)]
pub struct RrdIds;
impl IdProfile for RrdIds {
    type Error = RrdIdError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|value, metadata| validate_rrd_id(value, metadata.error_context));
}
