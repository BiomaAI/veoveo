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
enum MapReleaseState {
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
enum MapAcquisitionState {
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
enum MapRouteState {
    #[serde(rename = "planning_advisory")]
    #[surreal(value = "planning_advisory")]
    PlanningAdvisory,
    #[serde(rename = "validated")]
    #[surreal(value = "validated")]
    Validated,
    #[serde(rename = "stale")]
    #[surreal(value = "stale")]
    Stale,
    #[serde(rename = "invalidated")]
    #[surreal(value = "invalidated")]
    Invalidated,
    #[serde(rename = "unavailable")]
    #[surreal(value = "unavailable")]
    Unavailable,
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
enum MapDependencyKind {
    #[serde(rename = "release")]
    #[surreal(value = "release")]
    Release,
    #[serde(rename = "restriction")]
    #[surreal(value = "restriction")]
    Restriction,
    #[serde(rename = "facility")]
    #[surreal(value = "facility")]
    Facility,
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
        super::MapReleaseState::kind_of(),
        MapReleaseState::kind_of()
    );
    compare(
        super::MapReleaseState::Staged,
        MapReleaseState::Staged,
        "staged",
    );
    compare(
        super::MapReleaseState::Active,
        MapReleaseState::Active,
        "active",
    );
    compare(
        super::MapReleaseState::Retired,
        MapReleaseState::Retired,
        "retired",
    );
    compare(
        super::MapReleaseState::Quarantined,
        MapReleaseState::Quarantined,
        "quarantined",
    );
    assert_eq!(
        super::MapAcquisitionState::kind_of(),
        MapAcquisitionState::kind_of()
    );
    compare(
        super::MapAcquisitionState::Queued,
        MapAcquisitionState::Queued,
        "queued",
    );
    compare(
        super::MapAcquisitionState::Running,
        MapAcquisitionState::Running,
        "running",
    );
    compare(
        super::MapAcquisitionState::Succeeded,
        MapAcquisitionState::Succeeded,
        "succeeded",
    );
    compare(
        super::MapAcquisitionState::Failed,
        MapAcquisitionState::Failed,
        "failed",
    );
    compare(
        super::MapAcquisitionState::CancelRequested,
        MapAcquisitionState::CancelRequested,
        "cancel_requested",
    );
    compare(
        super::MapAcquisitionState::Cancelled,
        MapAcquisitionState::Cancelled,
        "cancelled",
    );
    assert_eq!(super::MapRouteState::kind_of(), MapRouteState::kind_of());
    compare(
        super::MapRouteState::PlanningAdvisory,
        MapRouteState::PlanningAdvisory,
        "planning_advisory",
    );
    compare(
        super::MapRouteState::Validated,
        MapRouteState::Validated,
        "validated",
    );
    compare(super::MapRouteState::Stale, MapRouteState::Stale, "stale");
    compare(
        super::MapRouteState::Invalidated,
        MapRouteState::Invalidated,
        "invalidated",
    );
    compare(
        super::MapRouteState::Unavailable,
        MapRouteState::Unavailable,
        "unavailable",
    );
    assert_eq!(
        super::MapDependencyKind::kind_of(),
        MapDependencyKind::kind_of()
    );
    compare(
        super::MapDependencyKind::Release,
        MapDependencyKind::Release,
        "release",
    );
    compare(
        super::MapDependencyKind::Restriction,
        MapDependencyKind::Restriction,
        "restriction",
    );
    compare(
        super::MapDependencyKind::Facility,
        MapDependencyKind::Facility,
        "facility",
    );
}
