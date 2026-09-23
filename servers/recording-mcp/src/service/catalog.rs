//! Resource discovery needs authorized identities, not playback or layer views.
use anyhow::Result;
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_platform_store::RecordingId;

use super::{RecordingService, record_uuid, visible};

pub struct RecordingResourceIdentity {
    pub recording_id: RecordingId,
    pub recording_key: String,
}

impl RecordingService {
    pub async fn list_resource_identities(
        &self,
        identity: &GatewayInternalIdentity,
    ) -> Result<Vec<RecordingResourceIdentity>> {
        let platform = self.platform_identity(identity).await?;
        self.store
            .list_recordings(platform.tenant_id, 500)
            .await?
            .into_iter()
            .filter(|record| visible(record, identity))
            .map(|record| {
                Ok(RecordingResourceIdentity {
                    recording_id: RecordingId::from_uuid(record_uuid(&record.id, "recording")?),
                    recording_key: record.recording_key,
                })
            })
            .collect()
    }
}
