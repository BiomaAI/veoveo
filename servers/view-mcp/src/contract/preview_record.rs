//! Checked preview manifest and affine tile transforms.
use super::*;
use glam::DMat4;

/// One preview tile with finite, invertible affine transform and derived size status.
///
/// ```compile_fail
/// use veoveo_view_mcp::SceneTileRecord;
/// fn corrupt(tile: &mut SceneTileRecord) { tile.oversize = false; }
/// ```
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[serde(try_from = "SceneTileRecordWire")]
pub struct SceneTileRecord(veoveo_types::Checked<SceneTileRecordWire>);

impl SceneTileRecord {
    pub fn new(
        tile_uri: TileUri,
        ecef_from_content: [f64; 16],
        byte_length: Option<u64>,
    ) -> Result<Self, PreviewSceneError> {
        Self::try_from(SceneTileRecordWire {
            tile_uri,
            ecef_from_content,
            byte_length,
            oversize: byte_length.is_some_and(|length| length > MAX_TILE_RESOURCE_BYTES),
        })
    }
}
impl veoveo_types::Check for SceneTileRecordWire {
    type Error = PreviewSceneError;
    fn check(&self) -> Result<(), Self::Error> {
        let value = self;
        let matrix = DMat4::from_cols_array(&value.ecef_from_content);
        let determinant = matrix.determinant();
        if !matrix.is_finite()
            || !determinant.is_finite()
            || determinant == 0.0
            || !matrix.inverse().is_finite()
            || value.ecef_from_content[3] != 0.0
            || value.ecef_from_content[7] != 0.0
            || value.ecef_from_content[11] != 0.0
            || value.ecef_from_content[15] != 1.0
        {
            return Err(PreviewSceneError::TileTransform);
        }
        if value.byte_length == Some(0)
            || value.oversize
                != value
                    .byte_length
                    .is_some_and(|length| length > MAX_TILE_RESOURCE_BYTES)
        {
            return Err(PreviewSceneError::TileSize);
        }
        Ok(())
    }
}
impl TryFrom<SceneTileRecordWire> for SceneTileRecord {
    type Error = PreviewSceneError;
    fn try_from(value: SceneTileRecordWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl Serialize for SceneTileRecord {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// The admitted view, render policy and preview cut share one immutable manifest.
///
/// ```compile_fail
/// use veoveo_view_mcp::PreviewSceneRecord;
/// fn corrupt(scene: &mut PreviewSceneRecord) { scene.view_revision = 0; }
/// ```
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[serde(try_from = "PreviewSceneRecordWire")]
pub struct PreviewSceneRecord(veoveo_types::Checked<PreviewSceneRecordWire>);
impl PreviewSceneRecord {
    pub fn new(
        view: &ViewRecord,
        policy: PreviewScenePolicy,
        detail_complete: bool,
        truncated: bool,
        attribution: AttributionSet,
        tiles: Vec<SceneTileRecord>,
    ) -> Result<Self, PreviewSceneError> {
        policy.validate(&super::RECORD_CAPTURE_LIMITS)?;
        let local_origin = view.resolved_camera().position;
        Self::try_from(PreviewSceneRecordWire {
            view_id: view.view_id().clone(),
            view_revision: view.revision(),
            composition_id: view.composition_id().clone(),
            composition_digest_sha256: view.composition_digest_sha256().clone(),
            scene_layer: view.scene_layer().clone(),
            resolved_camera: view.resolved_camera().clone(),
            local_origin,
            local_from_ecef: crate::geodesy::world_from_ecef(local_origin).to_cols_array(),
            width_px: policy.width_px,
            height_px: policy.height_px,
            max_screen_error_px: f64::from(policy.max_screen_error_px),
            detail_complete: detail_complete
                && !truncated
                && tiles.iter().all(|tile| !tile.oversize()),
            truncated,
            attribution,
            tiles,
        })
    }
}
impl veoveo_types::Check for PreviewSceneRecordWire {
    type Error = PreviewSceneError;
    fn check(&self) -> Result<(), Self::Error> {
        let value = self;
        if value.view_revision == 0 {
            return Err(PreviewSceneError::Revision);
        }
        value.resolved_camera.clone().validate()?;
        value.local_origin.validate()?;
        if value.local_origin != value.resolved_camera.position {
            return Err(PreviewSceneError::Origin);
        }
        let expected = crate::geodesy::world_from_ecef(value.local_origin).to_cols_array();
        // Cross-platform trigonometry may differ below a tenth of a millimetre.
        if value.local_from_ecef.iter().zip(expected).enumerate().any(
            |(index, (actual, expected))| {
                let tolerance = if (12..15).contains(&index) {
                    1e-4
                } else {
                    1e-12
                };
                !actual.is_finite() || (actual - expected).abs() > tolerance
            },
        ) {
            return Err(PreviewSceneError::Origin);
        }
        if value.width_px == 0
            || value.height_px == 0
            || !value.max_screen_error_px.is_finite()
            || !(0.25..=256.0).contains(&value.max_screen_error_px)
        {
            return Err(PreviewSceneError::Policy);
        }
        if value.tiles.len() > SCENE_MAX_TILES
            || (value.truncated && value.tiles.len() != SCENE_MAX_TILES)
        {
            return Err(PreviewSceneError::TileLimit);
        }
        if value.detail_complete
            && (value.truncated || value.tiles.iter().any(SceneTileRecord::oversize))
        {
            return Err(PreviewSceneError::Detail);
        }
        Ok(())
    }
}
impl TryFrom<PreviewSceneRecordWire> for PreviewSceneRecord {
    type Error = PreviewSceneError;
    fn try_from(value: PreviewSceneRecordWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl Serialize for PreviewSceneRecord {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum PreviewSceneError {
    #[error("preview view revision must be positive")]
    Revision,
    #[error("preview origin or local transform disagrees with its camera")]
    Origin,
    #[error("preview viewport or screen error is invalid")]
    Policy,
    #[error("preview tile transform must be finite, affine and invertible")]
    TileTransform,
    #[error("preview tile length or oversize status is inconsistent")]
    TileSize,
    #[error("preview tile count and truncation must respect the transport limit")]
    TileLimit,
    #[error("a truncated or oversize preview cannot declare complete detail")]
    Detail,
    #[error(transparent)]
    Contract(#[from] ContractError),
}
impl SceneTileRecord {
    pub fn tile_uri(&self) -> &TileUri {
        &self.0.tile_uri
    }
    pub fn ecef_from_content(&self) -> &[f64; 16] {
        &self.0.ecef_from_content
    }
    pub fn byte_length(&self) -> Option<u64> {
        self.0.byte_length
    }
    pub fn oversize(&self) -> bool {
        self.0.oversize
    }
}

impl PreviewSceneRecord {
    pub fn view_id(&self) -> &ViewId {
        &self.0.view_id
    }
    pub fn view_revision(&self) -> u64 {
        self.0.view_revision
    }
    pub fn composition_id(&self) -> &SceneCompositionId {
        &self.0.composition_id
    }
    pub fn composition_digest_sha256(&self) -> &Sha256Digest {
        &self.0.composition_digest_sha256
    }
    pub fn scene_layer(&self) -> &LayerId {
        &self.0.scene_layer
    }
    pub fn resolved_camera(&self) -> &GeodeticCameraPose {
        &self.0.resolved_camera
    }
    pub fn local_origin(&self) -> Wgs84Position3d {
        self.0.local_origin
    }
    pub fn local_from_ecef(&self) -> &[f64; 16] {
        &self.0.local_from_ecef
    }
    pub fn width_px(&self) -> u32 {
        self.0.width_px
    }
    pub fn height_px(&self) -> u32 {
        self.0.height_px
    }
    pub fn max_screen_error_px(&self) -> f64 {
        self.0.max_screen_error_px
    }
    pub fn detail_complete(&self) -> bool {
        self.0.detail_complete
    }
    pub fn truncated(&self) -> bool {
        self.0.truncated
    }
    pub fn attribution(&self) -> &AttributionSet {
        &self.0.attribution
    }
    pub fn tiles(&self) -> &[SceneTileRecord] {
        &self.0.tiles
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SceneTileRecordWire {
    tile_uri: TileUri,
    /// Column-major, meters (matches glam `to_cols_array` and three.js
    /// `Matrix4.fromArray`).
    ecef_from_content: [f64; 16],
    /// Raw GLB length when resident in the byte cache; absent after eviction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    byte_length: Option<u64>,
    /// Reads of oversize tiles fail; consumers must skip them.
    oversize: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PreviewSceneRecordWire {
    view_id: ViewId,
    view_revision: u64,
    composition_id: SceneCompositionId,
    composition_digest_sha256: Sha256Digest,
    scene_layer: LayerId,
    resolved_camera: GeodeticCameraPose,
    local_origin: Wgs84Position3d,
    /// Column-major local frame (+X east, +Y up, -Z north) from ECEF meters,
    /// anchored at `local_origin` so composed tile transforms stay
    /// scene-local and f32-safe.
    local_from_ecef: [f64; 16],
    width_px: u32,
    height_px: u32,
    max_screen_error_px: f64,
    detail_complete: bool,
    truncated: bool,
    attribution: AttributionSet,
    tiles: Vec<SceneTileRecord>,
}

#[cfg(test)]
mod tests;
