use super::{ResolvedSceneComposition, ResourceOwner};
use crate::contract::{CaptureFrameRequest, ViewRecord};
use serde::{Deserialize, Serialize};

/// Frozen, internally consistent capture input. Admission checks run on both
/// construction and decoding; the public accessors cannot mutate the snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[serde(try_from = "SnapshotWire")]
pub struct ViewCaptureSnapshot {
    view: ViewRecord,
    composition: ResolvedSceneComposition,
}

impl ViewCaptureSnapshot {
    pub fn new(
        view: ViewRecord,
        composition: ResolvedSceneComposition,
    ) -> Result<Self, ViewSnapshotError> {
        let record = composition.record();
        if view.composition_id() != record.composition_id()
            || view.composition_digest_sha256() != record.composition_digest_sha256()
            || view.scene_layer() != record.base_layer()
            || view.created_at() < record.created_at()
        {
            return Err(ViewSnapshotError::Composition);
        }
        Ok(Self { view, composition })
    }

    pub fn view(&self) -> &ViewRecord {
        &self.view
    }
    pub fn composition(&self) -> &ResolvedSceneComposition {
        &self.composition
    }

    pub fn require_owner(&self, owner: &ResourceOwner) -> Result<(), ViewSnapshotError> {
        let authority = self.composition.record().authority();
        if authority.principal_id != owner.principal_id
            || authority.invocation.work_context != owner.work_context
            || authority.invocation.tenant != owner.tenant
        {
            return Err(ViewSnapshotError::Owner);
        }
        Ok(())
    }

    pub fn require_request(&self, request: &CaptureFrameRequest) -> Result<(), ViewSnapshotError> {
        if &request.view_id != self.view.view_id()
            || request.expected_revision != self.view.revision()
        {
            return Err(ViewSnapshotError::Request);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SnapshotWire {
    view: ViewRecord,
    composition: ResolvedSceneComposition,
}

impl TryFrom<SnapshotWire> for ViewCaptureSnapshot {
    type Error = ViewSnapshotError;
    fn try_from(value: SnapshotWire) -> Result<Self, Self::Error> {
        Self::new(value.view, value.composition)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ViewSnapshotError {
    #[error("capture view disagrees with its immutable composition")]
    Composition,
    #[error("capture snapshot does not belong to the requested owner")]
    Owner,
    #[error("capture request does not name the saved view revision")]
    Request,
}

#[cfg(test)]
mod tests;
