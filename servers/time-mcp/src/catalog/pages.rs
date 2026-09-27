use super::{TimeAccessContext, TimeCatalog, event_from_record};
use crate::{
    contract::{CollectionPage, MissionEpoch, OperationalCalendar, TemporalEvent},
    index, uris,
};
use anyhow::{Context, Result};
use veoveo_platform_store::{TimeEventCursor, TimeTemporalEventState, TimeVersionCursor};

impl TimeCatalog {
    pub async fn calendars_page(
        &self,
        scope: &TimeAccessContext,
        after: Option<&TimeVersionCursor>,
    ) -> Result<CollectionPage<OperationalCalendar>> {
        let rows = self
            .store
            .list_time_calendar_versions(scope.identity.tenant_id, after, 101)
            .await?;
        index::page(
            rows,
            uris::CALENDARS_URI,
            |row| TimeVersionCursor {
                key: row.calendar_key.clone(),
                version: row.calendar_version,
            },
            |row| {
                serde_json::from_str(&row.canonical_json)
                    .context("decoding stored operational calendar")
            },
        )
    }
    pub async fn epochs_page(
        &self,
        scope: &TimeAccessContext,
        after: Option<&TimeVersionCursor>,
    ) -> Result<CollectionPage<MissionEpoch>> {
        let rows = self
            .store
            .list_time_mission_epochs(scope.identity.tenant_id, after, 101)
            .await?;
        index::page(
            rows,
            uris::EPOCHS_URI,
            |row| TimeVersionCursor {
                key: row.epoch_key.clone(),
                version: row.epoch_version,
            },
            |row| {
                serde_json::from_str(&row.canonical_json).context("decoding stored mission epoch")
            },
        )
    }
    pub async fn events_page(
        &self,
        scope: &TimeAccessContext,
        after: Option<&TimeEventCursor>,
        state: Option<TimeTemporalEventState>,
    ) -> Result<CollectionPage<TemporalEvent>> {
        let rows = self
            .store
            .list_time_temporal_events(&scope.identity, after, state, 101)
            .await?;
        index::page(
            rows,
            uris::EVENTS_URI,
            |row| TimeEventCursor {
                tai_seconds: row.due_tai_seconds_since_1970,
                nanosecond: row.due_nanosecond,
                event_key: row.event_key.clone(),
            },
            event_from_record,
        )
    }
    pub async fn epochs_for_keys(
        &self,
        scope: &TimeAccessContext,
        keys: &[String],
    ) -> Result<Vec<MissionEpoch>> {
        anyhow::ensure!(
            keys.len() <= 100_000,
            "at most 100000 epoch keys are supported"
        );
        let mut epochs = Vec::new();
        for keys in keys.chunks(100) {
            let rows = self
                .store
                .latest_time_mission_epochs(scope.identity.tenant_id, keys)
                .await?;
            for row in rows {
                epochs.push(
                    serde_json::from_str(&row.canonical_json)
                        .context("decoding stored mission epoch")?,
                );
            }
        }
        Ok(epochs)
    }
}
