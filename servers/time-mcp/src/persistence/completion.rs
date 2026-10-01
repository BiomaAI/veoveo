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
        let (table, field, predicate, parent_key) = match domain {
            TimeCompletion::CalendarId => ("time_calendar_version", "calendar_key", "true", None),
            TimeCompletion::CalendarVersion { calendar_key } => {
                if let Some(key) = calendar_key.as_ref() {
                    validate_key("calendar_key", key, "calendar-")?;
                }
                (
                    "time_calendar_version",
                    "type::string(calendar_version)",
                    "($parent_key = NONE OR calendar_key = $parent_key)",
                    calendar_key.map(|key| key.to_string()),
                )
            }
            TimeCompletion::EpochId => ("time_mission_epoch", "epoch_key", "true", None),
            TimeCompletion::EpochVersion { epoch_key } => (
                "time_mission_epoch",
                "type::string(epoch_version)",
                "($parent_key = NONE OR epoch_key = $parent_key)",
                epoch_key.map(|key| key.to_string()),
            ),
            TimeCompletion::AuthorityReleaseId => {
                ("time_authority_release", "release_key", "true", None)
            }
            TimeCompletion::EventId => ("time_temporal_event", "event_key", "owner = $owner", None),
        };
        // Only fixed repository-owned expressions enter the statement. All input is bound.
        let statement = format!(
            "SELECT VALUE candidate FROM (SELECT {field} AS candidate FROM {table} WHERE tenant = $tenant AND {predicate} AND string::lowercase({field}) CONTAINS $needle GROUP BY candidate ORDER BY candidate ASC LIMIT $limit);"
        );
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
