//! Catalog assembly from Store-authorized pages, without loading layer manifests.
use anyhow::{Context, Result};
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_platform_store::{
    PlatformIdentity, RecordingCursor, RecordingId, RecordingReadScope, RecordingRecord,
};
use veoveo_recording_reader::access::record_uuid;

use super::RecordingService;
use crate::{
    contract::RecordingCatalogPage,
    index::{self, PAGE_SIZE},
};

impl RecordingService {
    pub async fn catalog_page(
        &self,
        identity: &GatewayInternalIdentity,
        after: Option<&RecordingCursor>,
    ) -> Result<RecordingCatalogPage> {
        let platform = self.platform_identity(identity).await?;
        let scope = read_scope(&platform, identity);
        let mut records = self
            .store
            .list_recordings(&scope, after, PAGE_SIZE as u32 + 1)
            .await?;
        let has_more = records.len() > PAGE_SIZE;
        records.truncate(PAGE_SIZE);
        let next_cursor = if has_more {
            let last = records.last().context("missing Recording page cursor")?;
            Some(index::encode(RecordingCursor {
                started_at: last.started_at,
                recording_id: RecordingId::from_uuid(record_uuid(&last.id, "recording")?),
            })?)
        } else {
            None
        };
        let mut items = Vec::with_capacity(records.len());
        for record in records {
            items.push(self.view(platform.tenant_id, record).await?);
        }
        Ok(RecordingCatalogPage {
            items,
            limit: PAGE_SIZE,
            next_cursor,
        })
    }

    pub async fn complete_recording_ids(
        &self,
        identity: &GatewayInternalIdentity,
        needle: &str,
    ) -> Result<Vec<String>> {
        let platform = self.platform_identity(identity).await?;
        Ok(self
            .store
            .complete_recording_ids(
                &read_scope(&platform, identity),
                needle,
                PAGE_SIZE as u32 + 1,
            )
            .await?)
    }

    pub async fn visible_recording(
        &self,
        identity: &GatewayInternalIdentity,
        recording_id: RecordingId,
    ) -> Result<Option<(PlatformIdentity, RecordingRecord)>> {
        let platform = self.platform_identity(identity).await?;
        Ok(self
            .store
            .visible_recording(&read_scope(&platform, identity), recording_id)
            .await?
            .map(|recording| (platform, recording)))
    }
}

fn read_scope(
    platform: &PlatformIdentity,
    identity: &GatewayInternalIdentity,
) -> RecordingReadScope {
    RecordingReadScope {
        tenant_id: platform.tenant_id,
        data_labels: identity
            .actor
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect(),
    }
}
