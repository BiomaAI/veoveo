use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt,
};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{
    FrameWorldRevisionUri, InvocationAuthority, PrincipalId, WorldFrameUri,
    parse_artifact_plane_uri,
};

use super::{HeadingPitchRoll, LayerId, Wgs84Position3d};

pub const SCENE_COMPOSITION_SCHEMA_VERSION: u64 = 1;
pub const SCENE_COMPOSITION_ALGORITHM_REVISION: &str = "view-scene-composition-v1";
pub const OVERLAY_ARTIFACT_MIME_TYPE: &str = "application/vnd.veoveo.view-overlay-geometry+json";
pub const GLB_MIME_TYPE: &str = "model/gltf-binary";
pub const MAX_COMPOSITION_INPUTS: usize = 256;
pub const MAX_COMPOSITION_OVERLAYS: usize = 256;
pub const MAX_INLINE_OVERLAY_POINTS: usize = 4_096;
pub const MAX_ARTIFACT_OVERLAY_POINTS: usize = 100_000;
pub const MAX_INLINE_OVERLAY_BYTES: usize = 262_144;
pub const MAX_OVERLAY_ARTIFACT_BYTES: u64 = 16_777_216;
pub const MAX_COMPOSITION_ARTIFACT_BYTES: u64 = 67_108_864;
pub const MAX_LABEL_BYTES: usize = 64;

macro_rules! controlled_id {
    ($name:ident, $label:literal) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, SceneCompositionError> {
                let value = value.into();
                if value.is_empty()
                    || value.len() > 128
                    || value.trim() != value
                    || !value.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
                    })
                {
                    return Err(SceneCompositionError::InvalidIdentifier($label));
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = SceneCompositionError;

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

controlled_id!(SceneInputId, "scene input id");
controlled_id!(SceneOverlayId, "scene overlay id");
controlled_id!(SceneStyleId, "scene style id");

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct SceneCompositionId(String);

impl SceneCompositionId {
    const PREFIX: &'static str = "composition-";
    const NAMESPACE: u128 = 0x296b_036e_65dd_57b4_b309_3c47_b22e_419f;

    pub fn from_stable_key(value: &[u8]) -> Self {
        Self(format!(
            "{}{}",
            Self::PREFIX,
            uuid::Uuid::new_v5(&uuid::Uuid::from_u128(Self::NAMESPACE), value)
        ))
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, SceneCompositionError> {
        let value = value.into();
        let uuid = value
            .strip_prefix(Self::PREFIX)
            .and_then(|suffix| uuid::Uuid::parse_str(suffix).ok())
            .filter(|uuid| uuid.get_version_num() == 5)
            .ok_or(SceneCompositionError::InvalidIdentifier(
                "scene composition id",
            ))?;
        Ok(Self(format!("{}{}", Self::PREFIX, uuid)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SceneCompositionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for SceneCompositionId {
    type Error = SceneCompositionError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<SceneCompositionId> for String {
    fn from(value: SceneCompositionId) -> Self {
        value.0
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct Sha256Digest(String);

impl Sha256Digest {
    pub fn parse(value: impl Into<String>) -> Result<Self, SceneCompositionError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(SceneCompositionError::InvalidSha256);
        }
        Ok(Self(value))
    }

    pub fn from_bytes(value: &[u8]) -> Self {
        use sha2::{Digest as _, Sha256};
        Self(hex::encode(Sha256::digest(value)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for Sha256Digest {
    type Error = SceneCompositionError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<Sha256Digest> for String {
    fn from(value: Sha256Digest) -> Self {
        value.0
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct GovernedResourceUri(String);

impl GovernedResourceUri {
    pub fn parse(value: impl Into<String>) -> Result<Self, SceneCompositionError> {
        let value = value.into();
        let exact_resource = !value.is_empty()
            && value.len() <= 1_024
            && !value.contains(['?', '#'])
            && !value.chars().any(char::is_control)
            && (parse_artifact_plane_uri(&value).is_some()
                || [
                    "map://source-feature/",
                    "map://raster/",
                    "map://raster-derivation/",
                    "map://spatial-derivation/",
                    "map://route/",
                    "recording://recording/",
                    "recording://artifact/",
                    "frames://operation/",
                ]
                .iter()
                .any(|prefix| {
                    value
                        .strip_prefix(prefix)
                        .is_some_and(valid_resource_suffix)
                }));
        exact_resource
            .then_some(Self(value))
            .ok_or(SceneCompositionError::InvalidGovernedResourceUri)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_artifact(&self) -> bool {
        parse_artifact_plane_uri(&self.0).is_some()
    }
}

impl fmt::Display for GovernedResourceUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for GovernedResourceUri {
    type Error = SceneCompositionError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<GovernedResourceUri> for String {
    fn from(value: GovernedResourceUri) -> Self {
        value.0
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct MapReleaseUri(String);

impl MapReleaseUri {
    pub fn parse(value: impl Into<String>) -> Result<Self, SceneCompositionError> {
        let value = value.into();
        let valid = value
            .strip_prefix("map://dataset/")
            .and_then(|suffix| suffix.split_once("/release/"))
            .is_some_and(|(dataset, release)| {
                valid_uri_segment(dataset) && valid_uri_segment(release)
            });
        valid
            .then_some(Self(value))
            .ok_or(SceneCompositionError::InvalidMapReleaseUri)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MapReleaseUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for MapReleaseUri {
    type Error = SceneCompositionError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<MapReleaseUri> for String {
    fn from(value: MapReleaseUri) -> Self {
        value.0
    }
}

fn valid_uri_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !matches!(value, "." | "..")
        && !value.contains(['/', '?', '#'])
        && !value.chars().any(char::is_whitespace)
        && !value.chars().any(char::is_control)
}

fn valid_resource_suffix(value: &str) -> bool {
    !value.is_empty() && value.split('/').all(valid_uri_segment)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GovernedSceneInput {
    pub input_id: SceneInputId,
    pub resource_uri: GovernedResourceUri,
    pub digest_sha256: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    pub license: String,
    pub attribution: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LocalFrameBinding {
    pub world_revision: FrameWorldRevisionUri,
    pub frame_uri: WorldFrameUri,
    pub ecef_from_frame: [f64; 16],
    pub operation_input_id: SceneInputId,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScenePosition {
    Wgs84 { position: Wgs84Position3d },
    LocalMeters { xyz_meters: [f64; 3] },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OverlayColor {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl OverlayColor {
    pub const WHITE: Self = Self {
        red: 1.0,
        green: 1.0,
        blue: 1.0,
        alpha: 1.0,
    };

    pub const CYAN: Self = Self {
        red: 0.0,
        green: 0.85,
        blue: 1.0,
        alpha: 1.0,
    };

    pub fn as_array(self) -> [f32; 4] {
        [self.red, self.green, self.blue, self.alpha]
    }

    fn validate(self) -> Result<(), SceneCompositionError> {
        if [self.red, self.green, self.blue, self.alpha]
            .into_iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
        {
            Ok(())
        } else {
            Err(SceneCompositionError::InvalidStyle)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SceneOverlayStyle {
    #[serde(default = "default_stroke_color")]
    pub stroke_color: OverlayColor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<OverlayColor>,
    #[serde(default = "default_line_width")]
    pub line_width_meters: f64,
    #[serde(default = "default_marker_size")]
    pub marker_size_meters: f64,
    #[serde(default = "default_label_height")]
    pub label_height_meters: f64,
}

impl Default for SceneOverlayStyle {
    fn default() -> Self {
        Self {
            stroke_color: OverlayColor::CYAN,
            fill_color: None,
            line_width_meters: default_line_width(),
            marker_size_meters: default_marker_size(),
            label_height_meters: default_label_height(),
        }
    }
}

fn default_stroke_color() -> OverlayColor {
    OverlayColor::CYAN
}

fn default_line_width() -> f64 {
    2.0
}

fn default_marker_size() -> f64 {
    8.0
}

fn default_label_height() -> f64 {
    12.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SceneOverlayVisibility {
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_camera_distance_meters: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_camera_distance_meters: Option<f64>,
}

impl Default for SceneOverlayVisibility {
    fn default() -> Self {
        Self {
            visible: true,
            minimum_camera_distance_meters: None,
            maximum_camera_distance_meters: None,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SceneValidity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,
}

impl SceneValidity {
    pub fn contains(&self, scene_time: DateTime<Utc>) -> bool {
        self.valid_from.is_none_or(|start| scene_time >= start)
            && self.valid_until.is_none_or(|end| scene_time < end)
            && self
                .timestamp
                .is_none_or(|timestamp| timestamp == scene_time)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SceneOverlayGeometry {
    Marker {
        position: ScenePosition,
    },
    Polyline {
        positions: Vec<ScenePosition>,
        #[serde(default)]
        closed: bool,
    },
    Polygon {
        positions: Vec<ScenePosition>,
        triangle_indices: Vec<u32>,
    },
    OrientedMeshInstance {
        position: ScenePosition,
        orientation: HeadingPitchRoll,
        scale: [f64; 3],
        mesh_input_id: SceneInputId,
    },
    Label {
        position: ScenePosition,
        text: String,
    },
}

impl SceneOverlayGeometry {
    pub fn point_count(&self) -> usize {
        match self {
            Self::Marker { .. } | Self::OrientedMeshInstance { .. } | Self::Label { .. } => 1,
            Self::Polyline { positions, .. } | Self::Polygon { positions, .. } => positions.len(),
        }
    }

    pub fn positions(&self) -> Box<dyn Iterator<Item = &ScenePosition> + '_> {
        match self {
            Self::Marker { position }
            | Self::OrientedMeshInstance { position, .. }
            | Self::Label { position, .. } => Box::new(std::iter::once(position)),
            Self::Polyline { positions, .. } | Self::Polygon { positions, .. } => {
                Box::new(positions.iter())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SceneOverlayGeometrySource {
    Inline { geometry: SceneOverlayGeometry },
    Artifact { input_id: SceneInputId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SceneOverlay {
    pub overlay_id: SceneOverlayId,
    pub governed_input_ids: BTreeSet<SceneInputId>,
    pub geometry: SceneOverlayGeometrySource,
    #[serde(default)]
    pub style: SceneOverlayStyle,
    #[serde(default)]
    pub visibility: SceneOverlayVisibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validity: Option<SceneValidity>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateSceneCompositionRequest {
    pub schema_version: u64,
    pub base_layer: LayerId,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub map_releases: BTreeSet<MapReleaseUri>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_frame: Option<LocalFrameBinding>,
    pub style_id: SceneStyleId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub governed_inputs: Vec<GovernedSceneInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlays: Vec<SceneOverlay>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SceneCompositionAuthority {
    pub principal_id: PrincipalId,
    pub invocation: InvocationAuthority,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SceneComposition {
    pub schema_version: u64,
    pub composition_id: SceneCompositionId,
    pub composition_uri: String,
    pub revision: u64,
    pub base_layer: LayerId,
    pub map_releases: BTreeSet<MapReleaseUri>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_frame: Option<LocalFrameBinding>,
    pub style_id: SceneStyleId,
    pub governed_inputs: Vec<GovernedSceneInput>,
    pub overlays: Vec<SceneOverlay>,
    pub algorithm_revision: String,
    pub request_digest_sha256: Sha256Digest,
    pub composition_digest_sha256: Sha256Digest,
    pub authority: SceneCompositionAuthority,
    pub created_at: DateTime<Utc>,
}

impl CreateSceneCompositionRequest {
    pub fn validate(&self) -> Result<(), SceneCompositionError> {
        if self.schema_version != SCENE_COMPOSITION_SCHEMA_VERSION {
            return Err(SceneCompositionError::UnsupportedSchemaVersion);
        }
        if self.governed_inputs.len() > MAX_COMPOSITION_INPUTS {
            return Err(SceneCompositionError::InputLimit);
        }
        if self.overlays.len() > MAX_COMPOSITION_OVERLAYS {
            return Err(SceneCompositionError::OverlayLimit);
        }
        let input_ids = unique_ids(
            self.governed_inputs
                .iter()
                .map(|input| input.input_id.clone()),
            SceneCompositionError::DuplicateInput,
        )?;
        let input_by_id = self
            .governed_inputs
            .iter()
            .map(|input| (&input.input_id, input))
            .collect::<BTreeMap<_, _>>();
        for input in &self.governed_inputs {
            validate_text(&input.license, 256)?;
            validate_text(&input.attribution, 1_024)?;
            if let Some(media_type) = &input.media_type {
                validate_media_type(media_type)?;
            }
        }
        if self
            .governed_inputs
            .iter()
            .any(|input| input.resource_uri.as_str().starts_with("map://"))
            && self.map_releases.is_empty()
        {
            return Err(SceneCompositionError::MapReleaseRequired);
        }
        let overlay_ids = unique_ids(
            self.overlays
                .iter()
                .map(|overlay| overlay.overlay_id.clone()),
            SceneCompositionError::DuplicateOverlay,
        )?;
        debug_assert_eq!(input_ids.len(), self.governed_inputs.len());
        debug_assert_eq!(overlay_ids.len(), self.overlays.len());

        let mut inline_points = 0;
        let mut uses_local_positions = false;
        for overlay in &self.overlays {
            if overlay.governed_input_ids.is_empty()
                || !overlay
                    .governed_input_ids
                    .iter()
                    .all(|input| input_ids.contains(input))
            {
                return Err(SceneCompositionError::UnknownGovernedInput);
            }
            validate_style(&overlay.style)?;
            validate_visibility(&overlay.visibility)?;
            if let Some(validity) = &overlay.validity
                && validity
                    .valid_from
                    .zip(validity.valid_until)
                    .is_some_and(|(start, end)| end <= start)
            {
                return Err(SceneCompositionError::InvalidValidity);
            }
            match &overlay.geometry {
                SceneOverlayGeometrySource::Inline { geometry } => {
                    inline_points += geometry.point_count();
                    validate_geometry(geometry, &input_by_id, MAX_INLINE_OVERLAY_POINTS)?;
                    uses_local_positions |= geometry
                        .positions()
                        .any(|position| matches!(position, ScenePosition::LocalMeters { .. }));
                }
                SceneOverlayGeometrySource::Artifact { input_id } => {
                    let input = input_by_id
                        .get(input_id)
                        .ok_or(SceneCompositionError::UnknownGovernedInput)?;
                    if !input.resource_uri.is_artifact()
                        || input.media_type.as_deref() != Some(OVERLAY_ARTIFACT_MIME_TYPE)
                    {
                        return Err(SceneCompositionError::InvalidOverlayArtifactInput);
                    }
                }
            }
        }
        if inline_points > MAX_INLINE_OVERLAY_POINTS {
            return Err(SceneCompositionError::InlinePointLimit);
        }
        let inline_bytes =
            serde_json::to_vec(&self.overlays).map_err(|_| SceneCompositionError::Serialization)?;
        if inline_bytes.len() > MAX_INLINE_OVERLAY_BYTES {
            return Err(SceneCompositionError::InlineByteLimit);
        }
        match (&self.local_frame, uses_local_positions) {
            (None, true) => return Err(SceneCompositionError::LocalFrameRequired),
            (Some(binding), _) => validate_local_frame(binding, &input_by_id)?,
            (None, false) => {}
        }
        Ok(())
    }
}

pub fn validate_artifact_geometry(
    geometry: &SceneOverlayGeometry,
    inputs: &BTreeMap<&SceneInputId, &GovernedSceneInput>,
) -> Result<(), SceneCompositionError> {
    validate_geometry(geometry, inputs, MAX_ARTIFACT_OVERLAY_POINTS)
}

fn unique_ids<T: Eq + std::hash::Hash>(
    values: impl Iterator<Item = T>,
    error: SceneCompositionError,
) -> Result<HashSet<T>, SceneCompositionError> {
    let mut result = HashSet::new();
    for value in values {
        if !result.insert(value) {
            return Err(error);
        }
    }
    Ok(result)
}

fn validate_geometry(
    geometry: &SceneOverlayGeometry,
    inputs: &BTreeMap<&SceneInputId, &GovernedSceneInput>,
    point_limit: usize,
) -> Result<(), SceneCompositionError> {
    if geometry.point_count() == 0 || geometry.point_count() > point_limit {
        return Err(SceneCompositionError::GeometryPointLimit);
    }
    for position in geometry.positions() {
        validate_position(*position)?;
    }
    match geometry {
        SceneOverlayGeometry::Marker { .. } => {}
        SceneOverlayGeometry::Polyline {
            positions, closed, ..
        } => {
            if positions.len() < 2 || (*closed && positions.len() < 3) {
                return Err(SceneCompositionError::InvalidPolyline);
            }
        }
        SceneOverlayGeometry::Polygon {
            positions,
            triangle_indices,
        } => {
            if positions.len() < 3
                || triangle_indices.len() < 3
                || triangle_indices.len() % 3 != 0
                || triangle_indices
                    .iter()
                    .any(|index| *index as usize >= positions.len())
            {
                return Err(SceneCompositionError::InvalidPolygon);
            }
        }
        SceneOverlayGeometry::OrientedMeshInstance {
            orientation,
            scale,
            mesh_input_id,
            ..
        } => {
            orientation
                .validate()
                .map_err(|_| SceneCompositionError::InvalidMeshInstance)?;
            if !scale
                .iter()
                .all(|value| value.is_finite() && (0.000_001..=1_000_000.0).contains(value))
            {
                return Err(SceneCompositionError::InvalidMeshInstance);
            }
            let input = inputs
                .get(mesh_input_id)
                .ok_or(SceneCompositionError::UnknownGovernedInput)?;
            if !input.resource_uri.is_artifact()
                || input.media_type.as_deref() != Some(GLB_MIME_TYPE)
            {
                return Err(SceneCompositionError::InvalidMeshArtifactInput);
            }
        }
        SceneOverlayGeometry::Label { text, .. } => {
            if text.is_empty()
                || text.len() > MAX_LABEL_BYTES
                || !text
                    .bytes()
                    .all(|byte| byte.is_ascii_graphic() || byte == b' ')
            {
                return Err(SceneCompositionError::InvalidLabel);
            }
        }
    }
    Ok(())
}

fn validate_position(position: ScenePosition) -> Result<(), SceneCompositionError> {
    match position {
        ScenePosition::Wgs84 { position } => position
            .validate()
            .map(|_| ())
            .map_err(|_| SceneCompositionError::InvalidPosition),
        ScenePosition::LocalMeters { xyz_meters } => xyz_meters
            .into_iter()
            .all(|value| value.is_finite() && value.abs() <= 10_000_000.0)
            .then_some(())
            .ok_or(SceneCompositionError::InvalidPosition),
    }
}

fn validate_style(style: &SceneOverlayStyle) -> Result<(), SceneCompositionError> {
    style.stroke_color.validate()?;
    if let Some(fill) = style.fill_color {
        fill.validate()?;
    }
    if [
        style.line_width_meters,
        style.marker_size_meters,
        style.label_height_meters,
    ]
    .into_iter()
    .all(|value| value.is_finite() && (0.01..=100_000.0).contains(&value))
    {
        Ok(())
    } else {
        Err(SceneCompositionError::InvalidStyle)
    }
}

fn validate_visibility(visibility: &SceneOverlayVisibility) -> Result<(), SceneCompositionError> {
    let valid_bound = |value: Option<f64>| {
        value.is_none_or(|value| value.is_finite() && (0.0..=100_000_000.0).contains(&value))
    };
    if !valid_bound(visibility.minimum_camera_distance_meters)
        || !valid_bound(visibility.maximum_camera_distance_meters)
        || visibility
            .minimum_camera_distance_meters
            .zip(visibility.maximum_camera_distance_meters)
            .is_some_and(|(minimum, maximum)| maximum < minimum)
    {
        return Err(SceneCompositionError::InvalidVisibility);
    }
    Ok(())
}

fn validate_local_frame(
    binding: &LocalFrameBinding,
    inputs: &BTreeMap<&SceneInputId, &GovernedSceneInput>,
) -> Result<(), SceneCompositionError> {
    if !binding
        .frame_uri
        .as_str()
        .starts_with(&format!("{}/frame/", binding.world_revision))
        || !binding
            .ecef_from_frame
            .iter()
            .all(|value| value.is_finite())
        || binding.ecef_from_frame[3].abs() > f64::EPSILON
        || binding.ecef_from_frame[7].abs() > f64::EPSILON
        || binding.ecef_from_frame[11].abs() > f64::EPSILON
        || (binding.ecef_from_frame[15] - 1.0).abs() > f64::EPSILON
    {
        return Err(SceneCompositionError::InvalidLocalFrame);
    }
    let operation = inputs
        .get(&binding.operation_input_id)
        .ok_or(SceneCompositionError::UnknownGovernedInput)?;
    if !operation
        .resource_uri
        .as_str()
        .starts_with("frames://operation/")
    {
        return Err(SceneCompositionError::InvalidLocalFrame);
    }
    Ok(())
}

fn validate_text(value: &str, maximum: usize) -> Result<(), SceneCompositionError> {
    if value.is_empty()
        || value.len() > maximum
        || value.chars().any(char::is_control)
        || value.trim() != value
    {
        Err(SceneCompositionError::InvalidText)
    } else {
        Ok(())
    }
}

fn validate_media_type(value: &str) -> Result<(), SceneCompositionError> {
    if value.len() <= 128
        && value
            .split_once('/')
            .is_some_and(|(kind, subtype)| !kind.is_empty() && !subtype.is_empty())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'+' | b'-'))
    {
        Ok(())
    } else {
        Err(SceneCompositionError::InvalidMediaType)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SceneCompositionError {
    #[error("invalid {0}")]
    InvalidIdentifier(&'static str),
    #[error("SHA-256 digest must contain 64 lowercase hexadecimal characters")]
    InvalidSha256,
    #[error("governed resource must be one exact Map, Frames, Artifact, or Recording identity")]
    InvalidGovernedResourceUri,
    #[error("Map release URI must use map://dataset/{{dataset}}/release/{{release}}")]
    InvalidMapReleaseUri,
    #[error("governed Map inputs require at least one immutable Map release identity")]
    MapReleaseRequired,
    #[error("unsupported scene-composition schema version")]
    UnsupportedSchemaVersion,
    #[error("composition exceeds its governed-input limit")]
    InputLimit,
    #[error("composition exceeds its overlay limit")]
    OverlayLimit,
    #[error("duplicate governed input id")]
    DuplicateInput,
    #[error("duplicate overlay id")]
    DuplicateOverlay,
    #[error("overlay references an unknown governed input")]
    UnknownGovernedInput,
    #[error("inline overlay geometry exceeds the point limit")]
    InlinePointLimit,
    #[error("inline overlay geometry exceeds the byte limit; publish it as an artifact")]
    InlineByteLimit,
    #[error("overlay geometry exceeds its point limit")]
    GeometryPointLimit,
    #[error("artifact geometry input must be an exact artifact with the View overlay media type")]
    InvalidOverlayArtifactInput,
    #[error("mesh input must be an exact artifact with model/gltf-binary media type")]
    InvalidMeshArtifactInput,
    #[error("local positions require one valid Frames binding")]
    LocalFrameRequired,
    #[error("local Frames binding is invalid")]
    InvalidLocalFrame,
    #[error("scene position is invalid")]
    InvalidPosition,
    #[error("polyline needs at least two points and a closed line needs at least three")]
    InvalidPolyline,
    #[error("polygon needs bounded vertices and explicit triangle indices")]
    InvalidPolygon,
    #[error("oriented mesh instance is invalid")]
    InvalidMeshInstance,
    #[error("label must contain 1 to 64 printable ASCII bytes")]
    InvalidLabel,
    #[error("overlay style is outside configured contract bounds")]
    InvalidStyle,
    #[error("overlay visibility distance is invalid")]
    InvalidVisibility,
    #[error("overlay validity interval is invalid")]
    InvalidValidity,
    #[error("license or attribution text is invalid")]
    InvalidText,
    #[error("media type is invalid")]
    InvalidMediaType,
    #[error("composition serialization failed")]
    Serialization,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use veoveo_mcp_contract::{FrameId, FrameWorldId, FrameWorldRevisionId};

    fn position() -> ScenePosition {
        ScenePosition::Wgs84 {
            position: Wgs84Position3d {
                latitude_degrees: 13.7,
                longitude_degrees: -89.2,
                ellipsoidal_height_meters: 100.0,
            },
        }
    }

    fn input(id: &str, uri: &str, media_type: Option<&str>) -> GovernedSceneInput {
        GovernedSceneInput {
            input_id: SceneInputId::new(id).unwrap(),
            resource_uri: GovernedResourceUri::parse(uri).unwrap(),
            digest_sha256: Sha256Digest::from_bytes(id.as_bytes()),
            media_type: media_type.map(ToOwned::to_owned),
            license: "Test license".to_owned(),
            attribution: "Test source".to_owned(),
        }
    }

    #[test]
    fn composition_ids_and_digests_are_strong_wire_strings() {
        let id = SceneCompositionId::from_stable_key(b"composition");
        assert_eq!(SceneCompositionId::parse(id.to_string()).unwrap(), id);
        assert!(SceneCompositionId::parse("composition-nope").is_err());
        assert_eq!(Sha256Digest::from_bytes(b"x").as_str().len(), 64);
        assert!(GovernedResourceUri::parse("map://route/../route-1").is_err());
        assert!(GovernedResourceUri::parse("frames://operation/operation 1").is_err());
    }

    #[test]
    fn base_only_composition_is_valid() {
        let request = CreateSceneCompositionRequest {
            schema_version: SCENE_COMPOSITION_SCHEMA_VERSION,
            base_layer: LayerId::new("base").unwrap(),
            map_releases: BTreeSet::new(),
            local_frame: None,
            style_id: SceneStyleId::new("default:1").unwrap(),
            governed_inputs: Vec::new(),
            overlays: Vec::new(),
        };
        request.validate().unwrap();
    }

    #[test]
    fn local_positions_require_matching_frames_evidence() {
        let operation = input(
            "operation",
            "frames://operation/operation-1",
            Some("application/json"),
        );
        let mut request = CreateSceneCompositionRequest {
            schema_version: SCENE_COMPOSITION_SCHEMA_VERSION,
            base_layer: LayerId::new("base").unwrap(),
            map_releases: BTreeSet::new(),
            local_frame: None,
            style_id: SceneStyleId::new("default:1").unwrap(),
            governed_inputs: vec![operation],
            overlays: vec![SceneOverlay {
                overlay_id: SceneOverlayId::new("marker").unwrap(),
                governed_input_ids: BTreeSet::from([SceneInputId::new("operation").unwrap()]),
                geometry: SceneOverlayGeometrySource::Inline {
                    geometry: SceneOverlayGeometry::Marker {
                        position: ScenePosition::LocalMeters {
                            xyz_meters: [1.0, 2.0, 3.0],
                        },
                    },
                },
                style: SceneOverlayStyle::default(),
                visibility: SceneOverlayVisibility::default(),
                validity: None,
            }],
        };
        assert_eq!(
            request.validate().unwrap_err(),
            SceneCompositionError::LocalFrameRequired
        );
        let revision = FrameWorldRevisionUri::new(
            &FrameWorldId::new("world").unwrap(),
            &FrameWorldRevisionId::new("revision-1").unwrap(),
        );
        request.local_frame = Some(LocalFrameBinding {
            frame_uri: WorldFrameUri::new(&revision, &FrameId::new("local").unwrap()),
            world_revision: revision,
            ecef_from_frame: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 2.0, 3.0, 1.0,
            ],
            operation_input_id: SceneInputId::new("operation").unwrap(),
        });
        request.validate().unwrap();
    }

    #[test]
    fn artifact_geometry_requires_exact_media_type() {
        let artifact_id = veoveo_mcp_contract::ArtifactId::new();
        let request = CreateSceneCompositionRequest {
            schema_version: SCENE_COMPOSITION_SCHEMA_VERSION,
            base_layer: LayerId::new("base").unwrap(),
            map_releases: BTreeSet::new(),
            local_frame: None,
            style_id: SceneStyleId::new("default:1").unwrap(),
            governed_inputs: vec![input(
                "geometry",
                &artifact_id.plane_uri(),
                Some("application/json"),
            )],
            overlays: vec![SceneOverlay {
                overlay_id: SceneOverlayId::new("line").unwrap(),
                governed_input_ids: BTreeSet::from([SceneInputId::new("geometry").unwrap()]),
                geometry: SceneOverlayGeometrySource::Artifact {
                    input_id: SceneInputId::new("geometry").unwrap(),
                },
                style: SceneOverlayStyle::default(),
                visibility: SceneOverlayVisibility::default(),
                validity: None,
            }],
        };
        assert_eq!(
            request.validate().unwrap_err(),
            SceneCompositionError::InvalidOverlayArtifactInput
        );
    }

    #[test]
    fn governed_map_inputs_require_an_immutable_release() {
        let request = CreateSceneCompositionRequest {
            schema_version: SCENE_COMPOSITION_SCHEMA_VERSION,
            base_layer: LayerId::new("base").unwrap(),
            map_releases: BTreeSet::new(),
            local_frame: None,
            style_id: SceneStyleId::new("default:1").unwrap(),
            governed_inputs: vec![input("route", "map://route/route-1", None)],
            overlays: vec![SceneOverlay {
                overlay_id: SceneOverlayId::new("route").unwrap(),
                governed_input_ids: BTreeSet::from([SceneInputId::new("route").unwrap()]),
                geometry: SceneOverlayGeometrySource::Inline {
                    geometry: SceneOverlayGeometry::Marker {
                        position: position(),
                    },
                },
                style: SceneOverlayStyle::default(),
                visibility: SceneOverlayVisibility::default(),
                validity: None,
            }],
        };
        assert_eq!(
            request.validate().unwrap_err(),
            SceneCompositionError::MapReleaseRequired
        );
    }

    #[test]
    fn geometry_wire_shapes_are_tagged() {
        let geometry = SceneOverlayGeometry::Marker {
            position: position(),
        };
        let value = serde_json::to_value(geometry).unwrap();
        assert_eq!(value["kind"], json!("marker"));
    }
}
