//! Time-owned persistence over the shared Store connection.

use crate::catalog::TimeCompletion;
use crate::contract::{
    AuthorityReleaseId, CalendarId, ClockQualityPolicy, MissionEpochId, TemporalEventId,
    TimeAcquisitionId, TimeSourceId, TimeVersion, TimeVersionError, TimeWriteGuard,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_platform_store::{PlatformIdentity, PlatformStore, TenantId};

mod acquisitions;
mod authority;
mod calendars;
mod clock;
mod collections;
mod completion;
mod drafts;
mod events;
mod records;
mod validation;

pub(crate) use drafts::*;
pub(crate) use records::*;
pub(crate) use validation::validate_key;
use validation::*;

#[derive(Clone)]
pub(crate) struct TimePersistence {
    store: PlatformStore,
}

impl TimePersistence {
    pub(crate) fn new(store: PlatformStore) -> Self {
        Self { store }
    }

    fn client(&self) -> &surrealdb::Surreal<veoveo_platform_store::PlatformClient> {
        self.store.client()
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PersistenceError {
    #[error(transparent)]
    Version(#[from] TimeVersionError),
    #[error(transparent)]
    Database(#[from] surrealdb::Error),
    #[error("invalid time field {field}: {reason}")]
    InvalidTimeField {
        field: &'static str,
        reason: &'static str,
    },
    #[error("time {entity} `{key}` conflicts with the current durable record")]
    TimeRecordConflict { entity: &'static str, key: String },
    #[error("missing record during {operation}")]
    MissingRecord { operation: &'static str },
}

async fn create_only<T: SurrealValue>(
    store: &TimePersistence,
    record: RecordId,
    content: T,
) -> Result<(), PersistenceError> {
    store
        .client()
        .query("CREATE ONLY $record CONTENT $content RETURN NONE;")
        .bind(("record", record))
        .bind(("content", content))
        .await?
        .check()?;
    Ok(())
}

async fn select_one<T>(
    store: &TimePersistence,
    record: RecordId,
    tenant_id: TenantId,
) -> Result<Option<T>, PersistenceError>
where
    T: for<'de> Deserialize<'de> + SurrealValue,
{
    let mut response = store
        .client()
        .query("SELECT * FROM ONLY $record WHERE tenant = $tenant;")
        .bind(("record", record))
        .bind(("tenant", tenant_id.record_id()))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

async fn select_list<T>(
    store: &TimePersistence,
    query: &'static str,
    tenant_id: TenantId,
) -> Result<Vec<T>, PersistenceError>
where
    T: for<'de> Deserialize<'de> + SurrealValue,
{
    let mut response = store
        .client()
        .query(query)
        .bind(("tenant", tenant_id.record_id()))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

fn time_record(table: &'static str, key: impl AsRef<str>) -> RecordId {
    RecordId::new(table, key.as_ref().to_owned())
}

fn dataset_kind_key(kind: TimeDatasetKind) -> &'static str {
    match kind {
        TimeDatasetKind::Tzdb => "tzdb",
        TimeDatasetKind::LeapSeconds => "leap_seconds",
    }
}

fn invalid(field: &'static str, reason: &'static str) -> PersistenceError {
    PersistenceError::InvalidTimeField { field, reason }
}

fn conflict(entity: &'static str, key: String) -> PersistenceError {
    PersistenceError::TimeRecordConflict { entity, key }
}

#[cfg(test)]
mod activation_tests;
