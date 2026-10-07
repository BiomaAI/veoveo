use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    CameraDefinition, CompositionUri, ContractError, GeodeticCameraPose, LayerId, SceneComposition,
    SceneCompositionId, Sha256Digest, ViewId, ViewUri,
};

/// One immutable composition binding with a checked, replaceable camera revision.
///
/// ```compile_fail
/// use veoveo_view_mcp::ViewRecord;
/// fn change_parent(view: &mut ViewRecord) { view.revision = 0; }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[serde(try_from = "ViewRecordWire", into = "ViewRecordWire")]
pub struct ViewRecord {
    view_id: ViewId,
    composition_id: SceneCompositionId,
    composition_digest_sha256: Sha256Digest,
    scene_layer: LayerId,
    revision: u64,
    camera: CameraDefinition,
    resolved_camera: GeodeticCameraPose,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ViewRecord {
    pub fn new(
        view_id: ViewId,
        composition: &SceneComposition,
        camera: CameraDefinition,
        created_at: DateTime<Utc>,
    ) -> Result<Self, ViewRecordError> {
        if created_at < composition.created_at() {
            return Err(ViewRecordError::Timestamp);
        }
        let resolved_camera = crate::geodesy::resolve_camera(&camera)?.validate()?;
        Ok(Self {
            view_id,
            composition_id: composition.composition_id().clone(),
            composition_digest_sha256: composition.composition_digest_sha256().clone(),
            scene_layer: composition.base_layer().clone(),
            revision: 1,
            camera,
            resolved_camera,
            created_at,
            updated_at: created_at,
        })
    }

    /// Validate the complete replacement before changing the record.
    pub fn replace_camera(
        &mut self,
        camera: CameraDefinition,
        updated_at: DateTime<Utc>,
    ) -> Result<(), ViewRecordError> {
        let resolved_camera = crate::geodesy::resolve_camera(&camera)?.validate()?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(ViewRecordError::Revision)?;
        if updated_at < self.updated_at {
            return Err(ViewRecordError::Timestamp);
        }
        self.camera = camera;
        self.resolved_camera = resolved_camera;
        self.revision = revision;
        self.updated_at = updated_at;
        Ok(())
    }

    pub fn view_id(&self) -> &ViewId {
        &self.view_id
    }
    pub fn view_uri(&self) -> ViewUri {
        ViewUri::new(self.view_id.clone())
    }
    pub fn composition_id(&self) -> &SceneCompositionId {
        &self.composition_id
    }
    pub fn composition_uri(&self) -> CompositionUri {
        CompositionUri::new(self.composition_id.clone())
    }
    pub fn composition_digest_sha256(&self) -> &Sha256Digest {
        &self.composition_digest_sha256
    }
    pub fn scene_layer(&self) -> &LayerId {
        &self.scene_layer
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn camera(&self) -> &CameraDefinition {
        &self.camera
    }
    pub fn resolved_camera(&self) -> &GeodeticCameraPose {
        &self.resolved_camera
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ViewRecordWire {
    view_id: ViewId,
    view_uri: ViewUri,
    composition_id: SceneCompositionId,
    composition_uri: CompositionUri,
    composition_digest_sha256: Sha256Digest,
    scene_layer: LayerId,
    revision: u64,
    camera: CameraDefinition,
    resolved_camera: GeodeticCameraPose,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<ViewRecordWire> for ViewRecord {
    type Error = ViewRecordError;

    fn try_from(value: ViewRecordWire) -> Result<Self, Self::Error> {
        if value.view_uri != ViewUri::new(value.view_id.clone())
            || value.composition_uri != CompositionUri::new(value.composition_id.clone())
        {
            return Err(ViewRecordError::Identity);
        }
        if value.revision == 0 {
            return Err(ViewRecordError::Revision);
        }
        if value.updated_at < value.created_at {
            return Err(ViewRecordError::Timestamp);
        }
        value.resolved_camera.clone().validate()?;
        let resolved = crate::geodesy::resolve_camera(&value.camera)?.validate()?;
        let agrees = match &value.camera {
            CameraDefinition::Pose(_) => resolved == value.resolved_camera,
            _ => same_pose(&resolved, &value.resolved_camera),
        };
        if !agrees {
            return Err(ViewRecordError::Camera);
        }
        Ok(Self {
            view_id: value.view_id,
            composition_id: value.composition_id,
            composition_digest_sha256: value.composition_digest_sha256,
            scene_layer: value.scene_layer,
            revision: value.revision,
            camera: value.camera,
            resolved_camera: value.resolved_camera,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

// Transcendental functions can differ across CPUs/libm implementations. Preserve
// the saved pose after checking agreement within 1e-7 degrees and 0.1 mm height.
fn same_pose(left: &GeodeticCameraPose, right: &GeodeticCameraPose) -> bool {
    [
        (
            left.position.latitude_degrees,
            right.position.latitude_degrees,
        ),
        (
            left.position.longitude_degrees,
            right.position.longitude_degrees,
        ),
        (
            left.orientation.heading_degrees,
            right.orientation.heading_degrees,
        ),
        (
            left.orientation.pitch_degrees,
            right.orientation.pitch_degrees,
        ),
        (
            left.orientation.roll_degrees,
            right.orientation.roll_degrees,
        ),
    ]
    .into_iter()
    .all(|(left, right)| (left - right).abs() <= 1e-7)
        && (left.position.ellipsoidal_height_meters - right.position.ellipsoidal_height_meters)
            .abs()
            <= 1e-4
        && left.vertical_fov_degrees == right.vertical_fov_degrees
}

impl From<ViewRecord> for ViewRecordWire {
    fn from(value: ViewRecord) -> Self {
        Self {
            view_uri: value.view_uri(),
            composition_uri: value.composition_uri(),
            view_id: value.view_id,
            composition_id: value.composition_id,
            composition_digest_sha256: value.composition_digest_sha256,
            scene_layer: value.scene_layer,
            revision: value.revision,
            camera: value.camera,
            resolved_camera: value.resolved_camera,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ViewRecordError {
    #[error("view resource identities disagree")]
    Identity,
    #[error("view revision must be positive and must not overflow")]
    Revision,
    #[error("view timestamps must follow creation and camera revision order")]
    Timestamp,
    #[error("view resolved pose disagrees with its camera definition")]
    Camera,
    #[error(transparent)]
    Contract(#[from] ContractError),
}

#[cfg(test)]
mod tests;
