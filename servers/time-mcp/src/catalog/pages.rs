use super::{TimeAccessContext, TimeCatalog, event_from_record};
use crate::{
    contract::{
        CalendarCursor, CalendarId, CollectionPage, EpochCursor, EventCursor, MissionEpoch,
        MissionEpochId, OperationalCalendar, TemporalEvent, TemporalEventId, TimeVersion,
    },
    index,
};
use anyhow::{Context, Result};
use veoveo_platform_store::{TimeEventCursor, TimeTemporalEventState, TimeVersionCursor};

impl TimeCatalog {
    pub async fn calendars_page(
        &self,
        scope: &TimeAccessContext,
        after: Option<&CalendarCursor>,
    ) -> Result<CollectionPage<OperationalCalendar, CalendarCursor>> {
        let after = after.map(|cursor| TimeVersionCursor {
            key: cursor.calendar_id().to_string(),
            version: cursor.version().get() as i64,
        });
        let rows = self
            .store
            .list_time_calendar_versions(scope.identity.tenant_id, after.as_ref(), 101)
            .await?;
        index::page(
            rows,
            |row| {
                Ok(CalendarCursor::new(
                    &CalendarId::new(&row.calendar_key).map_err(anyhow::Error::msg)?,
                    TimeVersion::new(row.calendar_version.try_into()?)?,
                ))
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
        after: Option<&EpochCursor>,
    ) -> Result<CollectionPage<MissionEpoch, EpochCursor>> {
        let after = after.map(|cursor| TimeVersionCursor {
            key: cursor.epoch_id().to_string(),
            version: cursor.version().get() as i64,
        });
        let rows = self
            .store
            .list_time_mission_epochs(scope.identity.tenant_id, after.as_ref(), 101)
            .await?;
        index::page(
            rows,
            |row| {
                Ok(EpochCursor::new(
                    &MissionEpochId::new(&row.epoch_key).map_err(anyhow::Error::msg)?,
                    TimeVersion::new(row.epoch_version.try_into()?)?,
                ))
            },
            |row| {
                serde_json::from_str(&row.canonical_json).context("decoding stored mission epoch")
            },
        )
    }
    pub async fn events_page(
        &self,
        scope: &TimeAccessContext,
        after: Option<&EventCursor>,
        state: Option<TimeTemporalEventState>,
    ) -> Result<CollectionPage<TemporalEvent, EventCursor>> {
        let after = after.map(|cursor| TimeEventCursor {
            tai_seconds: cursor.tai_seconds(),
            nanosecond: i64::from(cursor.nanosecond()),
            event_key: cursor.event_id().to_string(),
        });
        let rows = self
            .store
            .list_time_temporal_events(&scope.identity, after.as_ref(), state, 101)
            .await?;
        index::page(
            rows,
            |row| {
                Ok(EventCursor::new(
                    &TemporalEventId::new(&row.event_key).map_err(anyhow::Error::msg)?,
                    row.due_tai_seconds_since_1970,
                    row.due_nanosecond.try_into()?,
                )?)
            },
            event_from_record,
        )
    }
    pub async fn epochs_for_keys(
        &self,
        scope: &TimeAccessContext,
        keys: &[MissionEpochId],
    ) -> Result<Vec<MissionEpoch>> {
        anyhow::ensure!(
            keys.len() <= 100_000,
            "at most 100000 epoch keys are supported"
        );
        let mut epochs = Vec::new();
        for keys in keys.chunks(100) {
            let keys: Vec<_> = keys.iter().map(ToString::to_string).collect();
            let rows = self
                .store
                .latest_time_mission_epochs(scope.identity.tenant_id, &keys)
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
