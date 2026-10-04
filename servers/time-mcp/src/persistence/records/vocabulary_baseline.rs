//! SDK value/serde declarations captured before the Vocabulary migration.

use surrealdb::types as surrealdb_types;
use surrealdb::types::SurrealValue;
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum TimeDatasetKind {
    #[serde(rename = "tzdb")]
    #[surreal(value = "tzdb")]
    Tzdb,
    #[serde(rename = "leap_seconds")]
    #[surreal(value = "leap_seconds")]
    LeapSeconds,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum TimeAuthorityReleaseState {
    #[serde(rename = "staged")]
    #[surreal(value = "staged")]
    Staged,
    #[serde(rename = "active")]
    #[surreal(value = "active")]
    Active,
    #[serde(rename = "retired")]
    #[surreal(value = "retired")]
    Retired,
    #[serde(rename = "quarantined")]
    #[surreal(value = "quarantined")]
    Quarantined,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum TimeAcquisitionState {
    #[serde(rename = "queued")]
    #[surreal(value = "queued")]
    Queued,
    #[serde(rename = "running")]
    #[surreal(value = "running")]
    Running,
    #[serde(rename = "succeeded")]
    #[surreal(value = "succeeded")]
    Succeeded,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
    #[serde(rename = "cancel_requested")]
    #[surreal(value = "cancel_requested")]
    CancelRequested,
    #[serde(rename = "cancelled")]
    #[surreal(value = "cancelled")]
    Cancelled,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum TimeCalendarState {
    #[serde(rename = "staged")]
    #[surreal(value = "staged")]
    Staged,
    #[serde(rename = "active")]
    #[surreal(value = "active")]
    Active,
    #[serde(rename = "retired")]
    #[surreal(value = "retired")]
    Retired,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum TimeTemporalEventState {
    #[serde(rename = "scheduled")]
    #[surreal(value = "scheduled")]
    Scheduled,
    #[serde(rename = "due")]
    #[surreal(value = "due")]
    Due,
    #[serde(rename = "cancelled")]
    #[surreal(value = "cancelled")]
    Cancelled,
}
fn compare<A, B>(actual: A, before: B, spelling: &str)
where
    A: Copy
        + std::fmt::Debug
        + PartialEq
        + SurrealValue
        + serde::Serialize
        + for<'de> serde::Deserialize<'de>
        + veoveo_types::Vocabulary,
    B: Copy + SurrealValue + serde::Serialize,
{
    let database = before.into_value();
    assert_eq!(actual.into_value(), database);
    assert_eq!(A::from_value(database.clone()).unwrap(), actual);
    assert_eq!(A::is_value(&database), B::is_value(&database));
    for invalid in [
        surrealdb::types::Value::String("unknown".into()),
        surrealdb::types::Value::None,
    ] {
        assert_eq!(A::is_value(&invalid), B::is_value(&invalid));
        assert_eq!(
            A::from_value(invalid.clone()).is_err(),
            B::from_value(invalid).is_err()
        );
    }
    let wire = serde_json::to_value(before).unwrap();
    assert_eq!(serde_json::to_value(actual).unwrap(), wire);
    assert_eq!(serde_json::from_value::<A>(wire).unwrap(), actual);
    assert_eq!(actual.as_str(), spelling);
}
#[test]
fn database_vocabulary_values_preserve_the_published_profile() {
    assert_eq!(
        super::TimeDatasetKind::kind_of(),
        TimeDatasetKind::kind_of()
    );
    compare(super::TimeDatasetKind::Tzdb, TimeDatasetKind::Tzdb, "tzdb");
    compare(
        super::TimeDatasetKind::LeapSeconds,
        TimeDatasetKind::LeapSeconds,
        "leap_seconds",
    );
    assert_eq!(
        super::TimeAuthorityReleaseState::kind_of(),
        TimeAuthorityReleaseState::kind_of()
    );
    compare(
        super::TimeAuthorityReleaseState::Staged,
        TimeAuthorityReleaseState::Staged,
        "staged",
    );
    compare(
        super::TimeAuthorityReleaseState::Active,
        TimeAuthorityReleaseState::Active,
        "active",
    );
    compare(
        super::TimeAuthorityReleaseState::Retired,
        TimeAuthorityReleaseState::Retired,
        "retired",
    );
    compare(
        super::TimeAuthorityReleaseState::Quarantined,
        TimeAuthorityReleaseState::Quarantined,
        "quarantined",
    );
    assert_eq!(
        super::TimeAcquisitionState::kind_of(),
        TimeAcquisitionState::kind_of()
    );
    compare(
        super::TimeAcquisitionState::Queued,
        TimeAcquisitionState::Queued,
        "queued",
    );
    compare(
        super::TimeAcquisitionState::Running,
        TimeAcquisitionState::Running,
        "running",
    );
    compare(
        super::TimeAcquisitionState::Succeeded,
        TimeAcquisitionState::Succeeded,
        "succeeded",
    );
    compare(
        super::TimeAcquisitionState::Failed,
        TimeAcquisitionState::Failed,
        "failed",
    );
    compare(
        super::TimeAcquisitionState::CancelRequested,
        TimeAcquisitionState::CancelRequested,
        "cancel_requested",
    );
    compare(
        super::TimeAcquisitionState::Cancelled,
        TimeAcquisitionState::Cancelled,
        "cancelled",
    );
    assert_eq!(
        super::TimeCalendarState::kind_of(),
        TimeCalendarState::kind_of()
    );
    compare(
        super::TimeCalendarState::Staged,
        TimeCalendarState::Staged,
        "staged",
    );
    compare(
        super::TimeCalendarState::Active,
        TimeCalendarState::Active,
        "active",
    );
    compare(
        super::TimeCalendarState::Retired,
        TimeCalendarState::Retired,
        "retired",
    );
    assert_eq!(
        super::TimeTemporalEventState::kind_of(),
        TimeTemporalEventState::kind_of()
    );
    compare(
        super::TimeTemporalEventState::Scheduled,
        TimeTemporalEventState::Scheduled,
        "scheduled",
    );
    compare(
        super::TimeTemporalEventState::Due,
        TimeTemporalEventState::Due,
        "due",
    );
    compare(
        super::TimeTemporalEventState::Cancelled,
        TimeTemporalEventState::Cancelled,
        "cancelled",
    );
}
