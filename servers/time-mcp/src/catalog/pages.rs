use super::{
    TimeAccessContext, TimeCatalog, calendar_from_record, epoch_from_record, event_from_record,
};
use crate::contract::TemporalEventState;
use crate::{
    contract::{
        CalendarCursor, CalendarId, CollectionPage, EpochCursor, EventCursor, MissionEpoch,
        MissionEpochId, OperationalCalendar, TemporalEvent, TemporalEventId, TimeVersion,
    },
    index,
};
use anyhow::Result;

impl TimeCatalog {
    pub async fn calendars_page(
        &self,
        scope: &TimeAccessContext,
        after: Option<&CalendarCursor>,
    ) -> Result<CollectionPage<OperationalCalendar, CalendarCursor>> {
        let rows = self
            .persistence
            .list_time_calendar_versions(scope.identity.tenant_id, after, 101)
            .await?;
        index::page(
            rows,
            |row| {
                Ok(CalendarCursor::new(
                    &CalendarId::new(&row.calendar_key).map_err(anyhow::Error::msg)?,
                    TimeVersion::new(row.calendar_version.try_into()?)?,
                ))
            },
            calendar_from_record,
        )
    }
    pub async fn epochs_page(
        &self,
        scope: &TimeAccessContext,
        after: Option<&EpochCursor>,
    ) -> Result<CollectionPage<MissionEpoch, EpochCursor>> {
        let rows = self
            .persistence
            .list_time_mission_epochs(scope.identity.tenant_id, after, 101)
            .await?;
        index::page(
            rows,
            |row| {
                Ok(EpochCursor::new(
                    &MissionEpochId::new(&row.epoch_key).map_err(anyhow::Error::msg)?,
                    TimeVersion::new(row.epoch_version.try_into()?)?,
                ))
            },
            epoch_from_record,
        )
    }
    pub async fn events_page(
        &self,
        scope: &TimeAccessContext,
        after: Option<&EventCursor>,
        state: Option<TemporalEventState>,
    ) -> Result<CollectionPage<TemporalEvent, EventCursor>> {
        let rows = self
            .persistence
            .list_time_temporal_events(&scope.identity, after, state.map(super::event_state), 101)
            .await?;
        index::page(
            rows,
            |row| {
                Ok(EventCursor::new(
                    &TemporalEventId::new(&row.event_key).map_err(anyhow::Error::msg)?,
                    row.due_tai_seconds_since_1970,
                    crate::SubsecondNanoseconds::new(row.due_nanosecond.try_into()?)?,
                ))
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
            let rows = self
                .persistence
                .latest_time_mission_epochs(scope.identity.tenant_id, keys)
                .await?;
            for row in rows {
                epochs.push(epoch_from_record(row)?);
            }
        }
        Ok(epochs)
    }
}
