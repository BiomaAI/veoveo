use super::*;

impl TimePersistence {
    pub(crate) async fn replace_time_clock_policy(
        &self,
        draft: TimeClockPolicyDraft,
        expected: TimeWriteGuard,
    ) -> Result<TimeClockPolicyRecord, PersistenceError> {
        let next = expected.next_version()?;
        let record = time_record("time_clock_policy", draft.identity.tenant_id.to_string());
        if expected == TimeWriteGuard::Absent {
            let now = Utc::now();
            let content = TimeClockPolicyContent {
                tenant: draft.identity.tenant_id.record_id(),
                owner: draft.identity.principal_id.record_id(),
                maximum_error_nanoseconds: draft.policy.maximum_error_nanoseconds() as i64,
                maximum_stratum: draft.policy.maximum_stratum() as i64,
                minimum_source_diversity: draft.policy.minimum_source_diversity() as i64,
                maximum_holdover_seconds: draft.policy.maximum_holdover_seconds() as i64,
                record_version: 1,
                created_at: now,
                updated_at: now,
            };
            create_only(self, record, content).await?;
        } else {
            let mut response = self
                .client()
                .query(include_str!("queries/update_clock_policy.surql"))
                .bind(("record", record))
                .bind(("owner", draft.identity.principal_id.record_id()))
                .bind(("tenant", draft.identity.tenant_id.record_id()))
                .bind((
                    "maximum_error",
                    draft.policy.maximum_error_nanoseconds() as i64,
                ))
                .bind(("maximum_stratum", draft.policy.maximum_stratum() as i64))
                .bind((
                    "minimum_diversity",
                    draft.policy.minimum_source_diversity() as i64,
                ))
                .bind((
                    "maximum_holdover",
                    draft.policy.maximum_holdover_seconds() as i64,
                ))
                .bind(("expected", expected.expected_version() as i64))
                .bind(("next", next.get() as i64))
                .await?
                .check()?;
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
