use super::*;

impl TimePersistence {
    pub(crate) async fn complete_time_values(
        &self,
        identity: &PlatformIdentity,
        domain: TimeCompletion,
        needle: &str,
        limit: u32,
    ) -> Result<Vec<String>, PersistenceError> {
        if !(1..=101).contains(&limit) {
            return Err(invalid("limit", "must be in 1..=101"));
        }
        let (statement, parent_key) = match domain {
            TimeCompletion::CalendarId => {
                (include_str!("queries/complete_calendar_id.surql"), None)
            }
            TimeCompletion::CalendarVersion { calendar_key } => {
                if let Some(key) = calendar_key.as_ref() {
                    validate_key("calendar_key", key, "calendar-")?;
                }
                (
                    include_str!("queries/complete_calendar_version.surql"),
                    calendar_key.map(|key| key.to_string()),
                )
            }
            TimeCompletion::EpochId => (include_str!("queries/complete_epoch_id.surql"), None),
            TimeCompletion::EpochVersion { epoch_key } => (
                include_str!("queries/complete_epoch_version.surql"),
                epoch_key.map(|key| key.to_string()),
            ),
            TimeCompletion::AuthorityReleaseId => (
                include_str!("queries/complete_authority_release_id.surql"),
                None,
            ),
            TimeCompletion::EventId => (include_str!("queries/complete_event_id.surql"), None),
        };
        let mut response = self
            .client()
            .query(statement)
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .bind(("parent_key", parent_key))
            .bind(("needle", needle.to_lowercase()))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}
