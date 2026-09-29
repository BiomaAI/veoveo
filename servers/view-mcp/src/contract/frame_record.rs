//! Checked capture metadata and byte ownership, independent of the renderer.
use super::*;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Immutable metadata for one admitted view/composition capture.
///
/// ```compile_fail
/// use veoveo_view_mcp::FrameRecord;
/// fn corrupt(frame: &mut FrameRecord) { frame.view_revision = 0; }
/// ```
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(try_from = "FrameRecordWire")]
pub struct FrameRecord(FrameRecordWire);

impl Serialize for FrameRecord {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
impl FrameRecord {
    pub fn mime_type(&self) -> &'static str {
        self.0.encoding.mime_type()
    }
    pub fn frame_id(&self) -> &FrameId {
        &self.0.frame_id
    }
    pub fn frame_uri(&self) -> &FrameUri {
        &self.0.frame_uri
    }
    pub fn view_id(&self) -> &ViewId {
        &self.0.view_id
    }
    pub fn view_revision(&self) -> u64 {
        self.0.view_revision
    }
    pub fn composition_id(&self) -> &SceneCompositionId {
        &self.0.composition_id
    }
    pub fn composition_uri(&self) -> &CompositionUri {
        &self.0.composition_uri
    }
    pub fn composition_revision(&self) -> u64 {
        self.0.composition_revision
    }
    pub fn composition_digest_sha256(&self) -> &Sha256Digest {
        &self.0.composition_digest_sha256
    }
    pub fn style_id(&self) -> &SceneStyleId {
        &self.0.style_id
    }
    pub fn governed_inputs(&self) -> &[GovernedSceneInput] {
        &self.0.governed_inputs
    }
    pub fn frame_world_revision(
        &self,
    ) -> Option<&veoveo_frames_mcp::contract::FrameWorldRevisionUri> {
        self.0.frame_world_revision.as_ref()
    }
    pub fn scene_layer(&self) -> &LayerId {
        &self.0.scene_layer
    }
    pub fn captured_at(&self) -> DateTime<Utc> {
        self.0.captured_at
    }
    pub fn scene_time(&self) -> DateTime<Utc> {
        self.0.scene_time
    }
    pub fn resolved_camera(&self) -> &GeodeticCameraPose {
        &self.0.resolved_camera
    }
    pub fn width_px(&self) -> u32 {
        self.0.width_px
    }
    pub fn height_px(&self) -> u32 {
        self.0.height_px
    }
    pub fn encoding(&self) -> FrameEncoding {
        self.0.encoding
    }
    pub fn byte_length(&self) -> u64 {
        self.0.byte_length
    }
    pub fn detail_complete(&self) -> bool {
        self.0.detail_complete
    }
    pub fn actual_max_screen_error_px(&self) -> f32 {
        self.0.actual_max_screen_error_px
    }
    pub fn visible_tile_count(&self) -> u32 {
        self.0.visible_tile_count
    }
    pub fn pending_tile_count(&self) -> u32 {
        self.0.pending_tile_count
    }
    pub fn rendered_overlay_count(&self) -> u32 {
        self.0.rendered_overlay_count
    }
    pub fn overlay_truncated(&self) -> bool {
        self.0.overlay_truncated
    }
    pub fn attribution(&self) -> &AttributionSet {
        &self.0.attribution
    }
    pub fn output_digest_sha256(&self) -> &Sha256Digest {
        &self.0.output_digest_sha256
    }
}

/// Achieved renderer detail. Admission checks finite error and completion consistency.
#[derive(Debug, Clone)]
pub struct FrameRenderReport {
    pub detail_complete: bool,
    pub actual_max_screen_error_px: f32,
    pub visible_tile_count: u32,
    pub pending_tile_count: u32,
    pub rendered_overlay_count: u32,
    pub overlay_truncated: bool,
    pub attribution: AttributionSet,
}

/// Bytes and metadata cannot be replaced independently after admission.
///
/// ```compile_fail
/// use veoveo_view_mcp::CapturedFrame;
/// fn corrupt(frame: &mut CapturedFrame) { frame.bytes.clear(); }
/// ```
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    record: FrameRecord,
    bytes: Vec<u8>,
}

impl CapturedFrame {
    pub fn builder<'a>(
        frame_id: FrameId,
        view: &'a ViewRecord,
        composition: &'a SceneComposition,
        scene_time: DateTime<Utc>,
        policy: &'a CapturePolicy,
    ) -> Result<FrameCaptureBuilder<'a>, FrameRecordError> {
        if view.composition_id() != composition.composition_id()
            || view.composition_digest_sha256() != composition.composition_digest_sha256()
            || view.scene_layer() != composition.base_layer()
            || view.created_at() < composition.created_at()
        {
            return Err(FrameRecordError::Parent);
        }
        policy.validate(&super::RECORD_CAPTURE_LIMITS)?;
        Ok(FrameCaptureBuilder {
            frame_id,
            view,
            composition,
            scene_time,
            policy,
        })
    }
    /// Check externally obtained bytes against an already admitted record.
    /// This verifies byte identity; image decoding belongs to the image consumer.
    pub fn from_record(record: FrameRecord, bytes: Vec<u8>) -> Result<Self, FrameRecordError> {
        if bytes.len() as u64 != record.byte_length()
            || Sha256Digest::from_bytes(&bytes) != *record.output_digest_sha256()
        {
            return Err(FrameRecordError::Bytes);
        }
        Ok(Self { record, bytes })
    }
    pub fn record(&self) -> &FrameRecord {
        &self.record
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn into_parts(self) -> (FrameRecord, Vec<u8>) {
        (self.record, self.bytes)
    }
}

pub struct FrameCaptureBuilder<'a> {
    frame_id: FrameId,
    view: &'a ViewRecord,
    composition: &'a SceneComposition,
    scene_time: DateTime<Utc>,
    policy: &'a CapturePolicy,
}

impl FrameCaptureBuilder<'_> {
    pub fn finish(
        self,
        captured_at: DateTime<Utc>,
        report: FrameRenderReport,
        encoding: FrameEncoding,
        bytes: Vec<u8>,
    ) -> Result<CapturedFrame, FrameRecordError> {
        if captured_at < self.view.updated_at() {
            return Err(FrameRecordError::Timestamp);
        }
        if encoding != self.policy.encoding {
            return Err(FrameRecordError::Encoding);
        }
        let attribution = report
            .attribution
            .lines
            .into_iter()
            .chain(
                self.composition
                    .governed_inputs()
                    .iter()
                    .map(|input| input.attribution.clone()),
            )
            .collect::<BTreeSet<_>>();
        let record = FrameRecord::try_from(FrameRecordWire {
            frame_uri: FrameUri::new(self.frame_id.clone()),
            frame_id: self.frame_id,
            view_id: self.view.view_id().clone(),
            view_revision: self.view.revision(),
            composition_id: self.composition.composition_id().clone(),
            composition_uri: self.composition.composition_uri().clone(),
            composition_revision: self.composition.revision(),
            composition_digest_sha256: self.composition.composition_digest_sha256().clone(),
            style_id: self.composition.style_id().clone(),
            governed_inputs: self.composition.governed_inputs().to_vec(),
            frame_world_revision: self
                .composition
                .local_frame()
                .map(|binding| binding.world_revision.clone()),
            scene_layer: self.view.scene_layer().clone(),
            captured_at,
            scene_time: self.scene_time,
            resolved_camera: self.view.resolved_camera().clone(),
            width_px: self.policy.width_px,
            height_px: self.policy.height_px,
            encoding,
            byte_length: bytes.len() as u64,
            detail_complete: report.detail_complete,
            actual_max_screen_error_px: report.actual_max_screen_error_px,
            visible_tile_count: report.visible_tile_count,
            pending_tile_count: report.pending_tile_count,
            rendered_overlay_count: report.rendered_overlay_count,
            overlay_truncated: report.overlay_truncated,
            attribution: AttributionSet {
                lines: attribution.into_iter().collect(),
            },
            output_digest_sha256: Sha256Digest::from_bytes(&bytes),
        })?;
        Ok(CapturedFrame { record, bytes })
    }
}

impl TryFrom<FrameRecordWire> for FrameRecord {
    type Error = FrameRecordError;
    fn try_from(value: FrameRecordWire) -> Result<Self, Self::Error> {
        if value.frame_uri != FrameUri::new(value.frame_id.clone())
            || value.composition_uri != CompositionUri::new(value.composition_id.clone())
        {
            return Err(FrameRecordError::Identity);
        }
        if value.view_revision == 0 || value.composition_revision != 1 {
            return Err(FrameRecordError::Revision);
        }
        if value.width_px == 0 || value.height_px == 0 {
            return Err(FrameRecordError::Viewport);
        }
        if value.byte_length == 0 {
            return Err(FrameRecordError::Bytes);
        }
        value.resolved_camera.clone().validate()?;
        super::composition::validate_governed_inputs(&value.governed_inputs)?;
        if value
            .governed_inputs
            .windows(2)
            .any(|pair| pair[0].input_id >= pair[1].input_id)
        {
            return Err(FrameRecordError::InputOrder);
        }
        if !value.actual_max_screen_error_px.is_finite()
            || value.actual_max_screen_error_px < 0.0
            || (value.detail_complete && value.pending_tile_count != 0)
        {
            return Err(FrameRecordError::Detail);
        }
        if value
            .attribution
            .lines
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
            || value.governed_inputs.iter().any(|input| {
                value
                    .attribution
                    .lines
                    .binary_search(&input.attribution)
                    .is_err()
            })
        {
            return Err(FrameRecordError::Attribution);
        }
        Ok(Self(value))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FrameRecordError {
    #[error("frame identities disagree")]
    Identity,
    #[error("frame view or composition revision is invalid")]
    Revision,
    #[error("capture view and composition disagree")]
    Parent,
    #[error("capture time precedes its view revision")]
    Timestamp,
    #[error("frame viewport must have positive dimensions")]
    Viewport,
    #[error("frame bytes are empty or disagree with length or digest")]
    Bytes,
    #[error("rendered image encoding disagrees with the capture policy")]
    Encoding,
    #[error("frame detail must be finite, nonnegative and consistent with pending tiles")]
    Detail,
    #[error("frame governed inputs must be ordered by identity")]
    InputOrder,
    #[error("frame attribution must be ordered, unique and include every governed input")]
    Attribution,
    #[error(transparent)]
    Contract(#[from] ContractError),
    #[error(transparent)]
    Composition(#[from] SceneCompositionError),
}

mod encoding_mime {
    use super::FrameEncoding;
    use serde::Deserialize;
    pub(super) fn serialize<S: serde::Serializer>(
        encoding: &FrameEncoding,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(encoding.mime_type())
    }
    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<FrameEncoding, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "image/png" => Ok(FrameEncoding::Png),
            "image/jpeg" => Ok(FrameEncoding::Jpeg),
            _ => Err(serde::de::Error::custom("unsupported frame media type")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct FrameRecordWire {
    frame_id: FrameId,
    frame_uri: FrameUri,
    view_id: ViewId,
    view_revision: u64,
    composition_id: SceneCompositionId,
    composition_uri: CompositionUri,
    composition_revision: u64,
    composition_digest_sha256: Sha256Digest,
    style_id: SceneStyleId,
    governed_inputs: Vec<GovernedSceneInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    frame_world_revision: Option<veoveo_frames_mcp::contract::FrameWorldRevisionUri>,
    scene_layer: LayerId,
    captured_at: DateTime<Utc>,
    scene_time: DateTime<Utc>,
    resolved_camera: GeodeticCameraPose,
    width_px: u32,
    height_px: u32,
    #[serde(rename = "mime_type", with = "encoding_mime")]
    #[schemars(schema_with = "encoding_schema")]
    encoding: FrameEncoding,
    byte_length: u64,
    detail_complete: bool,
    actual_max_screen_error_px: f32,
    visible_tile_count: u32,
    pending_tile_count: u32,
    rendered_overlay_count: u32,
    overlay_truncated: bool,
    attribution: AttributionSet,
    output_digest_sha256: Sha256Digest,
}

#[cfg(test)]
mod tests;

fn encoding_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type": "string", "enum": [FrameEncoding::Png.mime_type(), FrameEncoding::Jpeg.mime_type()]})
}
