use super::*;

impl TimePersistence {
    pub(crate) async fn replace_time_clock_policy(
        &self,
        draft: TimeClockPolicyDraft,
        expected: i64,
    ) -> Result<TimeClockPolicyRecord, PersistenceError> {
        validate_clock_policy(&draft)?;
        let record = time_record("time_clock_policy", draft.identity.tenant_id.to_string());
        if expected == 0 {
            let now = Utc::now();
            let content = TimeClockPolicyContent {
                tenant: draft.identity.tenant_id.record_id(),
                owner: draft.identity.principal_id.record_id(),
                maximum_error_nanoseconds: draft.maximum_error_nanoseconds,
                maximum_stratum: draft.maximum_stratum,
                minimum_source_diversity: draft.minimum_source_diversity,
                maximum_holdover_seconds: draft.maximum_holdover_seconds,
                record_version: 1,
                created_at: now,
                updated_at: now,
            };
            create_only(self, record, content).await?;
        } else {
            let mut response = self.client().query("UPDATE $record MERGE { owner: $owner, maximum_error_nanoseconds: $maximum_error, maximum_stratum: $maximum_stratum, minimum_source_diversity: $minimum_diversity, maximum_holdover_seconds: $maximum_holdover, record_version: $next, updated_at: time::now() } WHERE tenant = $tenant AND record_version = $expected RETURN AFTER;")
                .bind(("record", record)).bind(("owner", draft.identity.principal_id.record_id())).bind(("tenant", draft.identity.tenant_id.record_id())).bind(("maximum_error", draft.maximum_error_nanoseconds)).bind(("maximum_stratum", draft.maximum_stratum)).bind(("minimum_diversity", draft.minimum_source_diversity)).bind(("maximum_holdover", draft.maximum_holdover_seconds)).bind(("expected", expected)).bind(("next", expected + 1)).await?.check()?;
            if response.take::<Option<TimeClockPolicyRecord>>(0)?.is_none() {
                return Err(conflict(
                    "clock policy",
                    draft.identity.tenant_id.to_string(),
                ));
            }
        }
        self.time_clock_policy(draft.identity.tenant_id)
            .await?
            .ok_or(PersistenceError::MissingRecord {
                operation: "time clock policy readback",
            })
    }

    pub(crate) async fn time_clock_policy(
        &self,
        tenant_id: TenantId,
    ) -> Result<Option<TimeClockPolicyRecord>, PersistenceError> {
        select_one(
            self,
            time_record("time_clock_policy", tenant_id.to_string()),
            tenant_id,
        )
        .await
    }
}
