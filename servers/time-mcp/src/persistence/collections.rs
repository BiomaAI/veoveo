use super::*;
use crate::contract::{CalendarCursor, EpochCursor, EventCursor};

fn validate_limit(limit: u32) -> Result<(), PersistenceError> {
    if !(1..=101).contains(&limit) {
        return Err(invalid("limit", "must be in 1..=101"));
    }
    Ok(())
}

impl TimePersistence {
    pub(crate) async fn time_authority_releases_page(
        &self,
        tenant: TenantId,
        after: Option<&crate::AuthorityCursor>,
    ) -> Result<Vec<TimeAuthorityReleaseRecord>, PersistenceError> {
        let mut result = self
            .client()
            .query(include_str!("queries/release_page.surql"))
            .bind(("tenant", tenant.record_id()))
            .bind(("after", after.map(|cursor| cursor.release_id().to_string())))
            .await?
            .check()?;
        Ok(result.take(0)?)
    }
    pub(crate) async fn list_time_calendar_versions(
        &self,
        tenant_id: TenantId,
        after: Option<&CalendarCursor>,
        limit: u32,
    ) -> Result<Vec<TimeCalendarVersionRecord>, PersistenceError> {
        validate_limit(limit)?;
        if let Some(after) = after {
            validate_key("calendar_key", after.calendar_id(), "calendar-")?;
            validate_positive("calendar_version", after.version().get() as i64)?;
        }
        let mut response = self
            .client()
            .query(include_str!("queries/calendar_page.surql"))
            .bind(("tenant", tenant_id.record_id()))
            .bind((
                "after_key",
                after.map(|after| after.calendar_id().to_string()),
            ))
            .bind((
                "after_version",
                after.map(|after| after.version().get() as i64),
            ))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub(crate) async fn list_time_mission_epochs(
        &self,
        tenant_id: TenantId,
        after: Option<&EpochCursor>,
        limit: u32,
    ) -> Result<Vec<TimeMissionEpochRecord>, PersistenceError> {
        validate_limit(limit)?;
        if let Some(after) = after {
            validate_key("epoch_key", after.epoch_id(), "epoch-")?;
            validate_positive("epoch_version", after.version().get() as i64)?;
        }
        let mut response = self
            .client()
            .query(include_str!("queries/epoch_page.surql"))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("after_key", after.map(|after| after.epoch_id().to_string())))
            .bind((
                "after_version",
                after.map(|after| after.version().get() as i64),
            ))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub(crate) async fn list_time_temporal_events(
        &self,
        identity: &PlatformIdentity,
        after: Option<&EventCursor>,
        state: Option<TimeTemporalEventState>,
        limit: u32,
    ) -> Result<Vec<TimeTemporalEventRecord>, PersistenceError> {
        validate_limit(limit)?;
        if let Some(after) = after {
            validate_key("event_key", after.event_id(), "event-")?;
        }
        let mut response = self
            .client()
            .query(include_str!("queries/event_page.surql"))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .bind(("state", state))
            .bind(("after_key", after.map(|after| after.event_id().to_string())))
            .bind(("after_seconds", after.map(|after| after.tai_seconds())))
            .bind((
                "after_nanosecond",
                after.map(|after| i64::from(after.nanosecond().get())),
            ))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    /// Fetch only the latest versions of up to 100 explicitly requested epochs.
    pub(crate) async fn latest_time_mission_epochs(
        &self,
        tenant_id: TenantId,
        keys: &[MissionEpochId],
    ) -> Result<Vec<TimeMissionEpochRecord>, PersistenceError> {
        if keys.len() > 100 {
            return Err(invalid("epoch_keys", "must contain at most 100 keys"));
        }
        for key in keys {
            validate_key("epoch_key", key, "epoch-")?;
        }
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let mut response = self
            .client()
            .query(include_str!("queries/latest_epochs.surql"))
            .bind(("tenant", tenant_id.record_id()))
            .bind((
                "keys",
                keys.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}
