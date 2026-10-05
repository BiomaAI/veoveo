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
enum RecordingState {
    #[serde(rename = "live")]
    #[surreal(value = "live")]
    Live,
    #[serde(rename = "ready")]
    #[surreal(value = "ready")]
    Ready,
    #[serde(rename = "sealing")]
    #[surreal(value = "sealing")]
    Sealing,
    #[serde(rename = "sealed")]
    #[surreal(value = "sealed")]
    Sealed,
    #[serde(rename = "interrupted")]
    #[surreal(value = "interrupted")]
    Interrupted,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
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
enum RecordingRetentionMode {
    #[serde(rename = "installation_default")]
    #[surreal(value = "installation_default")]
    InstallationDefault,
    #[serde(rename = "retain_until")]
    #[surreal(value = "retain_until")]
    RetainUntil,
    #[serde(rename = "retain_forever")]
    #[surreal(value = "retain_forever")]
    RetainForever,
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
enum RecordingLayerKind {
    #[serde(rename = "capture")]
    #[surreal(value = "capture")]
    Capture,
    #[serde(rename = "properties")]
    #[surreal(value = "properties")]
    Properties,
    #[serde(rename = "derived")]
    #[surreal(value = "derived")]
    Derived,
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
enum RecordingLayerState {
    #[serde(rename = "writing")]
    #[surreal(value = "writing")]
    Writing,
    #[serde(rename = "staged")]
    #[surreal(value = "staged")]
    Staged,
    #[serde(rename = "committed")]
    #[surreal(value = "committed")]
    Committed,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
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
enum RecordingReadGrantClass {
    #[serde(rename = "viewer_segment")]
    #[surreal(value = "viewer_segment")]
    ViewerSegment,
    #[serde(rename = "catalog_dataset")]
    #[surreal(value = "catalog_dataset")]
    CatalogDataset,
    #[serde(rename = "app_projection")]
    #[surreal(value = "app_projection")]
    AppProjection,
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
enum RecordingProjectionState {
    #[serde(rename = "reserved")]
    #[surreal(value = "reserved")]
    Reserved,
    #[serde(rename = "materializing")]
    #[surreal(value = "materializing")]
    Materializing,
    #[serde(rename = "ready")]
    #[surreal(value = "ready")]
    Ready,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
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
enum RecordingIngestStreamState {
    #[serde(rename = "open")]
    #[surreal(value = "open")]
    Open,
    #[serde(rename = "finished")]
    #[surreal(value = "finished")]
    Finished,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
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
enum RecordingIngestBatchState {
    #[serde(rename = "durable")]
    #[surreal(value = "durable")]
    Durable,
    #[serde(rename = "materialized")]
    #[surreal(value = "materialized")]
    Materialized,
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
    assert_eq!(super::RecordingState::kind_of(), RecordingState::kind_of());
    compare(super::RecordingState::Live, RecordingState::Live, "live");
    compare(super::RecordingState::Ready, RecordingState::Ready, "ready");
    compare(
        super::RecordingState::Sealing,
        RecordingState::Sealing,
        "sealing",
    );
    compare(
        super::RecordingState::Sealed,
        RecordingState::Sealed,
        "sealed",
    );
    compare(
        super::RecordingState::Interrupted,
        RecordingState::Interrupted,
        "interrupted",
    );
    compare(
        super::RecordingState::Failed,
        RecordingState::Failed,
        "failed",
    );
    assert_eq!(
        super::RecordingRetentionMode::kind_of(),
        RecordingRetentionMode::kind_of()
    );
    compare(
        super::RecordingRetentionMode::InstallationDefault,
        RecordingRetentionMode::InstallationDefault,
        "installation_default",
    );
    compare(
        super::RecordingRetentionMode::RetainUntil,
        RecordingRetentionMode::RetainUntil,
        "retain_until",
    );
    compare(
        super::RecordingRetentionMode::RetainForever,
        RecordingRetentionMode::RetainForever,
        "retain_forever",
    );
    assert_eq!(
        super::RecordingLayerKind::kind_of(),
        RecordingLayerKind::kind_of()
    );
    compare(
        super::RecordingLayerKind::Capture,
        RecordingLayerKind::Capture,
        "capture",
    );
    compare(
        super::RecordingLayerKind::Properties,
        RecordingLayerKind::Properties,
        "properties",
    );
    compare(
        super::RecordingLayerKind::Derived,
        RecordingLayerKind::Derived,
        "derived",
    );
    assert_eq!(
        super::RecordingLayerState::kind_of(),
        RecordingLayerState::kind_of()
    );
    compare(
        super::RecordingLayerState::Writing,
        RecordingLayerState::Writing,
        "writing",
    );
    compare(
        super::RecordingLayerState::Staged,
        RecordingLayerState::Staged,
        "staged",
    );
    compare(
        super::RecordingLayerState::Committed,
        RecordingLayerState::Committed,
        "committed",
    );
    compare(
        super::RecordingLayerState::Failed,
        RecordingLayerState::Failed,
        "failed",
    );
    assert_eq!(
        super::RecordingReadGrantClass::kind_of(),
        RecordingReadGrantClass::kind_of()
    );
    compare(
        super::RecordingReadGrantClass::ViewerSegment,
        RecordingReadGrantClass::ViewerSegment,
        "viewer_segment",
    );
    compare(
        super::RecordingReadGrantClass::CatalogDataset,
        RecordingReadGrantClass::CatalogDataset,
        "catalog_dataset",
    );
    compare(
        super::RecordingReadGrantClass::AppProjection,
        RecordingReadGrantClass::AppProjection,
        "app_projection",
    );
    assert_eq!(
        super::RecordingProjectionState::kind_of(),
        RecordingProjectionState::kind_of()
    );
    compare(
        super::RecordingProjectionState::Reserved,
        RecordingProjectionState::Reserved,
        "reserved",
    );
    compare(
        super::RecordingProjectionState::Materializing,
        RecordingProjectionState::Materializing,
        "materializing",
    );
    compare(
        super::RecordingProjectionState::Ready,
        RecordingProjectionState::Ready,
        "ready",
    );
    compare(
        super::RecordingProjectionState::Failed,
        RecordingProjectionState::Failed,
        "failed",
    );
    compare(
        super::RecordingProjectionState::Cancelled,
        RecordingProjectionState::Cancelled,
        "cancelled",
    );
    assert_eq!(
        super::RecordingIngestStreamState::kind_of(),
        RecordingIngestStreamState::kind_of()
    );
    compare(
        super::RecordingIngestStreamState::Open,
        RecordingIngestStreamState::Open,
        "open",
    );
    compare(
        super::RecordingIngestStreamState::Finished,
        RecordingIngestStreamState::Finished,
        "finished",
    );
    compare(
        super::RecordingIngestStreamState::Failed,
        RecordingIngestStreamState::Failed,
        "failed",
    );
    assert_eq!(
        super::RecordingIngestBatchState::kind_of(),
        RecordingIngestBatchState::kind_of()
    );
    compare(
        super::RecordingIngestBatchState::Durable,
        RecordingIngestBatchState::Durable,
        "durable",
    );
    compare(
        super::RecordingIngestBatchState::Materialized,
        RecordingIngestBatchState::Materialized,
        "materialized",
    );
}
