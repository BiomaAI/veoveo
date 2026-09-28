use super::{instant, scope};
use crate::{catalog::TimeCatalog, contract::*, test_store::TestDb};
use chrono::Utc;
use serde_json::json;
use std::{collections::BTreeMap, time::Duration};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_platform_store::PlatformStore;

pub(super) async fn set(
    store: &PlatformStore,
    record: &RecordId,
    field: &str,
    value: impl SurrealValue,
) {
    // Deliberate corruption through the fixture writer, bypassing catalog admission.
    store
        .client()
        .query("UPDATE $record MERGE $patch RETURN NONE;")
        .bind(("record", record.clone()))
        .bind((
            "patch",
            BTreeMap::from([(field.to_owned(), value.into_value())]),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
}

async fn body(store: &PlatformStore, record: &RecordId) -> String {
    store
        .client()
        .query("SELECT VALUE canonical_json FROM ONLY $record;")
        .bind(("record", record.clone()))
        .await
        .unwrap()
        .check()
        .unwrap()
        .take::<Option<String>>(0)
        .unwrap()
        .unwrap()
}

fn corrupt(original: &str, field: &str, value: serde_json::Value) -> String {
    let mut json: serde_json::Value = serde_json::from_str(original).unwrap();
    json[field] = value;
    serde_json::to_string(&json).unwrap()
}

#[tokio::test]
async fn collection_reads_reject_identity_and_ordering_conflicts_after_sql_visibility() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let owner = scope(&db.a, "time-metadata", "owner").await;
        let peer = scope(&db.a, "time-metadata", "peer").await;
        let foreign = scope(&db.a, "time-metadata-other", "owner").await;
        let catalog = TimeCatalog::new(db.b.clone());
        let calendar = catalog
            .create_calendar(
                &owner,
                OperationalCalendar {
                    calendar_id: CalendarId::new("calendar-00000000-0000-7000-8000-000000000001")
                        .unwrap(),
                    version: 1,
                    name: "operations".into(),
                    zone_id: "UTC".into(),
                    windows: vec![],
                    excluded_dates: vec![],
                },
            )
            .await
            .unwrap();
        let record = RecordId::new(
            "time_calendar_version",
            format!("{}:1", calendar.calendar_id),
        );
        let original = body(&db.a, &record).await;
        for (field, value) in [
            (
                "calendar_id",
                json!("calendar-00000000-0000-7000-8000-000000000002"),
            ),
            ("version", json!(2)),
            ("name", json!("other")),
            ("zone_id", json!("Europe/London")),
        ] {
            let bad = corrupt(&original, field, value);
            set(&db.a, &record, "canonical_json", bad.clone()).await;
            assert!(
                catalog
                    .calendar(&owner, &calendar.calendar_id, TimeVersion::new(1).unwrap())
                    .await
                    .is_err(),
                "{field}"
            );
            assert!(
                catalog.calendars_page(&peer, None).await.is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .calendar(
                        &foreign,
                        &calendar.calendar_id,
                        TimeVersion::new(1).unwrap()
                    )
                    .await
                    .unwrap()
                    .is_none()
            );
            assert!(
                catalog
                    .calendars_page(&foreign, None)
                    .await
                    .unwrap()
                    .items
                    .is_empty()
            );
            assert_eq!(
                body(&db.a, &record).await,
                bad,
                "reads must leave retained data unchanged"
            );
        }
        // Matching body/column versions cannot override the actual record key.
        set(
            &db.a,
            &record,
            "canonical_json",
            corrupt(&original, "version", json!(2)),
        )
        .await;
        set(&db.a, &record, "calendar_version", 2_i64).await;
        assert!(
            catalog
                .calendars_page(&owner, None)
                .await
                .unwrap_err()
                .to_string()
                .contains("record_id")
        );
        set(&db.a, &record, "calendar_version", 1_i64).await;
        set(&db.a, &record, "canonical_json", original.clone()).await;
        assert_eq!(
            catalog.calendars_page(&owner, None).await.unwrap().items,
            vec![calendar]
        );

        let epoch = catalog
            .create_epoch(
                &owner,
                MissionEpoch {
                    epoch_id: MissionEpochId::new("epoch-00000000-0000-7000-8000-000000000001")
                        .unwrap(),
                    name: "mission".into(),
                    instant: instant(),
                    version: 1,
                },
            )
            .await
            .unwrap();
        let record = RecordId::new("time_mission_epoch", format!("{}:1", epoch.epoch_id));
        let original = body(&db.a, &record).await;
        let mut wrong_instant = instant();
        wrong_instant.tai_seconds_since_1970 += 1;
        for (field, value) in [
            (
                "epoch_id",
                json!("epoch-00000000-0000-7000-8000-000000000002"),
            ),
            ("version", json!(2)),
            ("instant", json!(wrong_instant)),
        ] {
            let bad = corrupt(&original, field, value);
            set(&db.a, &record, "canonical_json", bad.clone()).await;
            assert!(
                catalog.epoch(&owner, &epoch.epoch_id).await.is_err(),
                "{field}"
            );
            assert!(catalog.epochs_page(&owner, None).await.is_err(), "{field}");
            assert!(
                catalog
                    .epochs_for_keys(&peer, std::slice::from_ref(&epoch.epoch_id))
                    .await
                    .is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .epochs_for_keys(&foreign, std::slice::from_ref(&epoch.epoch_id))
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(body(&db.a, &record).await, bad);
        }
        set(&db.a, &record, "canonical_json", original).await;
        assert_eq!(
            catalog
                .epochs_for_keys(&owner, std::slice::from_ref(&epoch.epoch_id))
                .await
                .unwrap(),
            vec![epoch]
        );

        let event = catalog
            .create_event(
                &owner,
                TemporalEvent {
                    event_id: TemporalEventId::new("event-00000000-0000-7000-8000-000000000001")
                        .unwrap(),
                    name: "launch".into(),
                    due: instant(),
                    state: TemporalEventState::Scheduled,
                    record_version: 1,
                },
                "launch".into(),
            )
            .await
            .unwrap();
        let record = RecordId::new("time_temporal_event", event.event_id.to_string());
        let original = body(&db.a, &record).await;
        let mut wrong_due = instant();
        wrong_due.nanosecond += 1;
        for (field, value) in [
            (
                "event_id",
                json!("event-00000000-0000-7000-8000-000000000002"),
            ),
            ("name", json!("other")),
            ("due", json!(wrong_due)),
        ] {
            let bad = corrupt(&original, field, value);
            set(&db.a, &record, "canonical_json", bad.clone()).await;
            assert!(
                catalog.event(&owner, &event.event_id).await.is_err(),
                "{field}"
            );
            assert!(
                catalog.events_page(&owner, None, None).await.is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .cancel_event(&owner, &event.event_id, crate::TimeVersion::FIRST)
                    .await
                    .is_err(),
                "{field}"
            );
            for denied in [&peer, &foreign] {
                assert!(
                    catalog
                        .event(denied, &event.event_id)
                        .await
                        .unwrap()
                        .is_none()
                );
                assert!(
                    catalog
                        .events_page(denied, None, None)
                        .await
                        .unwrap()
                        .items
                        .is_empty()
                );
            }
            assert_eq!(body(&db.a, &record).await, bad);
        }
        let mut invalid_due = instant();
        invalid_due.nanosecond = 1_000_000_000;
        set(
            &db.a,
            &record,
            "canonical_json",
            corrupt(&original, "due", json!(invalid_due)),
        )
        .await;
        set(&db.a, &record, "due_nanosecond", 1_000_000_000_i64).await;
        assert!(
            catalog
                .event(&owner, &event.event_id)
                .await
                .unwrap_err()
                .to_string()
                .contains("due_nanosecond")
        );
        set(&db.a, &record, "due_nanosecond", 17_i64).await;
        set(&db.a, &record, "canonical_json", original).await;
        for version in [0_i64, -1] {
            set(&db.a, &record, "record_version", version).await;
            assert!(
                catalog
                    .event(&owner, &event.event_id)
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("record_version")
            );
        }
        set(&db.a, &record, "record_version", 1_i64).await;
        assert_eq!(
            catalog
                .cancel_event(&owner, &event.event_id, crate::TimeVersion::FIRST)
                .await
                .unwrap()
                .state,
            TemporalEventState::Cancelled
        );
    })
    .await
    .expect("Time collection metadata qualification exceeded 90 seconds");
}

#[tokio::test]
async fn administrative_metadata_preserves_lifecycle_columns_and_rejects_conflicting_bodies() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let owner = scope(&db.a, "time-authority-metadata", "owner").await;
        let foreign = scope(&db.a, "time-authority-metadata-other", "owner").await;
        let catalog = TimeCatalog::new(db.b.clone());
        let source = catalog
            .create_source(
                &owner,
                TimeSource {
                    source_id: TimeSourceId::new(
                        "time-source-00000000-0000-7000-8000-000000000001",
                    )
                    .unwrap(),
                    name: "IANA".into(),
                    dataset_kind: AuthorityDatasetKind::LeapSeconds,
                    url: "https://example.test/leaps".into(),
                    expected_content_type: "text/plain".into(),
                    enabled: true,
                    record_version: 1,
                },
            )
            .await
            .unwrap();
        let record = RecordId::new("time_source", source.source_id.to_string());
        let original = body(&db.a, &record).await;
        for (field, value) in [
            (
                "source_id",
                json!("time-source-00000000-0000-7000-8000-000000000002"),
            ),
            ("dataset_kind", json!("tzdb")),
            ("url", json!("https://example.test/other")),
            ("expected_content_type", json!("application/json")),
            ("enabled", json!(false)),
            ("dataset_kind", json!("STORED_SECRET_MARKER")),
        ] {
            let bad = corrupt(&original, field, value);
            set(&db.a, &record, "canonical_json", bad.clone()).await;
            let error = catalog.source(&owner, &source.source_id).await.unwrap_err();
            assert!(!format!("{error:#}").contains("STORED_SECRET_MARKER"));
            assert!(catalog.list_sources(&owner).await.is_err(), "{field}");
            assert!(
                catalog
                    .source(&foreign, &source.source_id)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(body(&db.a, &record).await, bad);
        }
        set(&db.a, &record, "canonical_json", original).await;

        let now = Utc::now();
        let release = catalog
            .create_release(
                &owner,
                AuthorityRelease {
                    release_id: AuthorityReleaseId::new(
                        "time-release-00000000-0000-7000-8000-000000000001",
                    )
                    .unwrap(),
                    source_id: source.source_id.clone(),
                    dataset_kind: source.dataset_kind,
                    version_label: "2026".into(),
                    source_url: source.url.clone(),
                    source_digest_sha256: "a".repeat(64),
                    artifact_path: "/var/lib/veoveo/time/releases/fixture/leaps".into(),
                    state: AuthorityReleaseState::Staged,
                    retrieved_at: now,
                    validated_at: now,
                    record_version: 1,
                },
            )
            .await
            .unwrap();
        catalog
            .activate_release(
                &owner,
                &release.release_id,
                crate::TimeVersion::FIRST,
                crate::TimeWriteGuard::new(0).unwrap(),
            )
            .await
            .unwrap();
        let record = RecordId::new("time_authority_release", release.release_id.to_string());
        let active_body = body(&db.a, &record).await;
        let mut second = release.clone();
        second.release_id =
            AuthorityReleaseId::new("time-release-00000000-0000-7000-8000-000000000002").unwrap();
        second.source_digest_sha256 = "b".repeat(64);
        let second = catalog.create_release(&owner, second).await.unwrap();
        catalog
            .activate_release(
                &owner,
                &second.release_id,
                crate::TimeVersion::FIRST,
                crate::TimeWriteGuard::new(1).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(body(&db.a, &record).await, active_body);
        let retired = catalog
            .release(&owner, &release.release_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retired.state, AuthorityReleaseState::Retired);
        assert_eq!(retired.record_version, 3);
        for (field, value) in [
            ("release_id", json!(second.release_id)),
            (
                "source_id",
                json!("time-source-00000000-0000-7000-8000-000000000002"),
            ),
            ("dataset_kind", json!("tzdb")),
            ("source_digest_sha256", json!("c".repeat(64))),
            ("version_label", json!("other")),
            ("artifact_path", json!("/other")),
        ] {
            let bad = corrupt(&active_body, field, value);
            set(&db.a, &record, "canonical_json", bad.clone()).await;
            assert!(
                catalog.release(&owner, &release.release_id).await.is_err(),
                "{field}"
            );
            assert!(catalog.list_releases(&owner).await.is_err(), "{field}");
            assert!(
                catalog
                    .release(&foreign, &release.release_id)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(body(&db.a, &record).await, bad);
        }
        set(&db.a, &record, "canonical_json", active_body).await;

        let acquisition = catalog
            .create_acquisition(
                &owner,
                TimeAcquisition {
                    acquisition_id: TimeAcquisitionId::new(
                        "time-acquisition-00000000-0000-7000-8000-000000000001",
                    )
                    .unwrap(),
                    source_id: source.source_id.clone(),
                    expected_source_digest_sha256: Some("a".repeat(64)),
                    status: TimeAcquisitionStatus::Queued,
                    phase: "queued".into(),
                    staged_release_id: Some(release.release_id.clone()),
                    message: "".into(),
                    created_at: now,
                    updated_at: now,
                    record_version: 1,
                },
                "acquire".into(),
            )
            .await
            .unwrap();
        let record = RecordId::new("time_acquisition", acquisition.acquisition_id.to_string());
        let original = body(&db.a, &record).await;
        for (field, value) in [
            (
                "acquisition_id",
                json!("time-acquisition-00000000-0000-7000-8000-000000000002"),
            ),
            (
                "source_id",
                json!("time-source-00000000-0000-7000-8000-000000000002"),
            ),
            ("expected_source_digest_sha256", json!("b".repeat(64))),
        ] {
            let bad = corrupt(&original, field, value);
            set(&db.a, &record, "canonical_json", bad.clone()).await;
            assert!(
                catalog
                    .acquisition(&owner, &acquisition.acquisition_id)
                    .await
                    .is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .acquisition_for_idempotency(&owner, "acquire")
                    .await
                    .is_err()
            );
            assert!(
                catalog
                    .acquisition_for_release(&owner, &release.release_id)
                    .await
                    .is_err()
            );
            assert!(catalog.list_acquisitions(&owner).await.is_err());
            assert!(
                catalog
                    .acquisition(&foreign, &acquisition.acquisition_id)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(body(&db.a, &record).await, bad);
        }
        set(&db.a, &record, "canonical_json", original.clone()).await;
        let mut wrong_source = acquisition.clone();
        wrong_source.source_id =
            TimeSourceId::new("time-source-00000000-0000-7000-8000-000000000002").unwrap();
        let mut wrong_digest = acquisition.clone();
        wrong_digest.expected_source_digest_sha256 = None;
        let mut wrong_creation = acquisition.clone();
        wrong_creation.created_at += chrono::Duration::seconds(1);
        for wrong in [wrong_source, wrong_digest, wrong_creation] {
            assert!(catalog.update_acquisition(&owner, wrong).await.is_err());
            assert_eq!(
                body(&db.a, &record).await,
                original,
                "invalid updates must not mutate storage"
            );
        }
        let mut updated = acquisition.clone();
        updated.status = TimeAcquisitionStatus::Succeeded;
        updated.phase = "complete".into();
        let updated = catalog.update_acquisition(&owner, updated).await.unwrap();
        assert_eq!(updated.record_version, 2);
        let other = TimeCatalog::new(db.a.clone());
        let mut left = updated.clone();
        left.phase = "left".into();
        let mut right = updated;
        right.phase = "right".into();
        let (left, right) = tokio::join!(
            catalog.update_acquisition(&owner, left),
            other.update_acquisition(&owner, right),
        );
        assert_ne!(left.is_ok(), right.is_ok(), "one version fence may advance");
        let winner = left.or(right).unwrap();
        assert_eq!(winner.record_version, 3);
        // Historical bodies may lag mutable lifecycle columns.
        set(&db.a, &record, "canonical_json", original).await;
        let restored = catalog
            .acquisition(&owner, &acquisition.acquisition_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(restored.status, TimeAcquisitionStatus::Succeeded);
        assert_eq!(restored.phase, winner.phase);
        assert_eq!(restored.record_version, 3);
        set(
            &db.a,
            &record,
            "staged_release_key",
            "time-release-bootstrap".to_owned(),
        )
        .await;
        assert!(
            catalog
                .acquisition(&owner, &acquisition.acquisition_id)
                .await
                .unwrap_err()
                .to_string()
                .contains("staged_release_key")
        );
    })
    .await
    .expect("Time administrative metadata qualification exceeded 90 seconds");
}
