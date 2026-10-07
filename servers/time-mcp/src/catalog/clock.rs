use super::{TimeAccessContext, TimeCatalog};
use crate::persistence::{TimeClockPolicyDraft, TimeClockPolicyRecord};
use crate::{ClockPolicyField, ClockQualityPolicy, TimeVersion, TimeWriteGuard};
use anyhow::Result;
use surrealdb::types::RecordId;
use veoveo_platform_store::TenantId;

impl TimeCatalog {
    pub async fn clock_policy(
        &self,
        scope: &TimeAccessContext,
    ) -> Result<Option<(ClockQualityPolicy, TimeVersion)>> {
        self.persistence
            .time_clock_policy(scope.identity.tenant_id)
            .await?
            .map(|record| decode(scope.identity.tenant_id, record))
            .transpose()
    }

    pub async fn replace_clock_policy(
        &self,
        scope: &TimeAccessContext,
        policy: ClockQualityPolicy,
        expected: TimeWriteGuard,
    ) -> Result<(ClockQualityPolicy, TimeVersion)> {
        let record = self
            .persistence
            .replace_time_clock_policy(
                TimeClockPolicyDraft {
                    identity: scope.identity.clone(),
                    policy,
                },
                expected,
            )
            .await?;
        decode(scope.identity.tenant_id, record)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid stored Time clock policy in {field}; inspect the retained record")]
struct StoredClockPolicyError {
    field: &'static str,
}

fn invalid(field: &'static str) -> anyhow::Error {
    StoredClockPolicyError { field }.into()
}

fn unsigned<T: TryFrom<i64>>(field: &'static str, value: i64) -> Result<T> {
    T::try_from(value).map_err(|_| invalid(field))
}

fn decode(
    tenant: TenantId,
    record: TimeClockPolicyRecord,
) -> Result<(ClockQualityPolicy, TimeVersion)> {
    if record.id != RecordId::new("time_clock_policy", tenant.to_string())
        || record.tenant != tenant.record_id()
    {
        return Err(invalid("identity"));
    }
    let policy = ClockQualityPolicy::builder()
        .maximum_error_nanoseconds(unsigned(
            "maximum_error_nanoseconds",
            record.maximum_error_nanoseconds,
        )?)
        .maximum_stratum(unsigned("maximum_stratum", record.maximum_stratum)?)
        .minimum_source_diversity(unsigned(
            "minimum_source_diversity",
            record.minimum_source_diversity,
        )?)
        .maximum_holdover_seconds(unsigned(
            "maximum_holdover_seconds",
            record.maximum_holdover_seconds,
        )?)
        .build()
        .map_err(|error| {
            invalid(match error.field() {
                ClockPolicyField::MaximumErrorNanoseconds => "maximum_error_nanoseconds",
                ClockPolicyField::MaximumStratum => "maximum_stratum",
                ClockPolicyField::MinimumSourceDiversity => "minimum_source_diversity",
                ClockPolicyField::MaximumHoldoverSeconds => "maximum_holdover_seconds",
            })
        })?;
    let version = TimeVersion::new(unsigned("record_version", record.record_version)?)
        .map_err(|_| invalid("record_version"))?;
    Ok((policy, version))
}
