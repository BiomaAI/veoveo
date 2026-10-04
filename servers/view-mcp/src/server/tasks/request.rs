use crate::{
    contract::CaptureFrameRequest,
    state::{ResourceOwner, ViewCaptureSnapshot, ViewSnapshotError},
};
use serde::{Deserialize, Serialize};
use veoveo_task_runtime::TaskOwner;
use veoveo_types::PrincipalId;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "RequestWire")]
pub(super) struct ViewCaptureTaskRequest {
    request: CaptureFrameRequest,
    view_snapshot: ViewCaptureSnapshot,
}

impl ViewCaptureTaskRequest {
    pub(super) fn new(
        request: CaptureFrameRequest,
        view_snapshot: ViewCaptureSnapshot,
    ) -> Result<Self, ViewSnapshotError> {
        view_snapshot.require_request(&request)?;
        Ok(Self {
            request,
            view_snapshot,
        })
    }

    pub(super) fn request(&self) -> &CaptureFrameRequest {
        &self.request
    }
    pub(super) fn snapshot(&self) -> &ViewCaptureSnapshot {
        &self.view_snapshot
    }

    pub(super) fn validate_owner(&self, owner: &TaskOwner) -> anyhow::Result<ResourceOwner> {
        let resource_owner = ResourceOwner {
            principal_id: PrincipalId::parse(owner.principal_key.clone())?,
            work_context: owner.authority.work_context.clone(),
            tenant: owner.authority.tenant.clone(),
        };
        self.view_snapshot.require_owner(&resource_owner)?;
        Ok(resource_owner)
    }

    pub(super) fn into_parts(self) -> (CaptureFrameRequest, ViewCaptureSnapshot) {
        (self.request, self.view_snapshot)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RequestWire {
    request: CaptureFrameRequest,
    view_snapshot: ViewCaptureSnapshot,
}

impl TryFrom<RequestWire> for ViewCaptureTaskRequest {
    type Error = ViewSnapshotError;
    fn try_from(value: RequestWire) -> Result<Self, Self::Error> {
        Self::new(value.request, value.view_snapshot)
    }
}

#[cfg(test)]
mod tests;
