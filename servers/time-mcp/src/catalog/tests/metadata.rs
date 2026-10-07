use super::{instant, scope};
use crate::{catalog::TimeCatalog, contract::*};
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
        .query(include_str!("../../tests/queries/merge_record.surql"))
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
        .query(include_str!(
            "../../tests/queries/read_canonical_json.surql"
        ))
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
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let owner = scope(&db.a, "time-metadata", "owner").await;
        let peer = scope(&db.a, "time-metadata", "peer").await;
        let foreign = scope(&db.a, "time-metadata-other", "owner").await;
        let catalog = TimeCatalog::new(db.b.clone());
        let calendar = catalog
            .create_calendar(
                &owner,
                crate::OperationalCalendarValue {
                    calendar_id: CalendarId::parse("calendar-00000000-0000-7000-8000-000000000001")
                        .unwrap(),
                    version: crate::TimeVersion::new(1).unwrap(),
                    name: "operations".into(),
                    zone_id: "UTC".into(),
                    windows: vec![],
                    excluded_dates: vec![],
                }
                .build()
                .unwrap(),
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
                "calendarId",
                json!("calendar-00000000-0000-7000-8000-000000000002"),
            ),
            ("version", json!(2)),
            ("name", json!("other")),
            ("zoneId", json!("Europe/London")),
        ] {
            let bad = corrupt(&original, field, value);
            serde_json::from_str::<crate::OperationalCalendar>(&bad)
                .expect("selected calendar corruption stays portable-admitted");
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
                    epoch_id: MissionEpochId::parse("epoch-00000000-0000-7000-8000-000000000001")
                        .unwrap(),
                    name: "mission".into(),
                    instant: instant(),
                    version: crate::TimeVersion::new(1).unwrap(),
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
                "epochId",
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
                    event_id: TemporalEventId::parse("event-00000000-0000-7000-8000-000000000001")
                        .unwrap(),
                    name: "launch".into(),
                    due: instant(),
                    state: TemporalEventState::Scheduled,
                    record_version: crate::TimeVersion::new(1).unwrap(),
                },
                "launch".into(),
            )
            .await
            .unwrap();
        let record = RecordId::new("time_temporal_event", event.event_id.to_string());
        let original = body(&db.a, &record).await;
        let mut wrong_due = instant();
        wrong_due.nanosecond =
            crate::SubsecondNanoseconds::new(wrong_due.nanosecond.get() + 1).unwrap();
        for (field, value) in [
            (
                "eventId",
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
        let mut invalid_due = json!(instant());
        invalid_due["nanosecond"] = 1_000_000_000.into();
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
                .contains("canonical_json")
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
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let owner = scope(&db.a, "time-authority-metadata", "owner").await;
        let foreign = scope(&db.a, "time-authority-metadata-other", "owner").await;
        let catalog = TimeCatalog::new(db.b.clone());
        let source = catalog
            .create_source(
                &owner,
                crate::NewTimeSourceValue {
                    source_id: TimeSourceId::parse(
                        "time-source-00000000-0000-7000-8000-000000000001",
                    )
                    .unwrap(),
                    name: "IANA".into(),
                    dataset_kind: AuthorityDatasetKind::LeapSeconds,
                    url: "https://example.test/leaps".into(),
                    expected_content_type: "text/plain".into(),
                    enabled: true,
                    record_version: crate::SourceCreationVersion,
                }
                .build()
                .unwrap(),
            )
            .await
            .unwrap();
        let record = RecordId::new("time_source", source.source_id.to_string());
        let original = body(&db.a, &record).await;
        for (field, value) in [
            (
                "sourceId",
                json!("time-source-00000000-0000-7000-8000-000000000002"),
            ),
            ("datasetKind", json!("tzdb")),
            ("url", json!("https://example.test/other")),
            ("expectedContentType", json!("application/json")),
            ("enabled", json!(false)),
            ("datasetKind", json!("STORED_SECRET_MARKER")),
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
                crate::AuthorityReleaseValue {
                    release_id: AuthorityReleaseId::parse(
                        "time-release-00000000-0000-7000-8000-000000000001",
                    )
                    .unwrap(),
                    source_id: source.source_id.clone(),
                    dataset_kind: source.dataset_kind,
                    version_label: "2026".into(),
                    source_url: source.url.clone(),
                    source_digest_sha256: "a".repeat(64).parse().unwrap(),
                    artifact_path: "/var/lib/veoveo/time/releases/fixture/leaps".into(),
                    state: AuthorityReleaseState::Staged,
                    retrieved_at: now,
                    validated_at: now,
                    record_version: crate::TimeVersion::new(1).unwrap(),
                }
                .build()
                .unwrap(),
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
        let mut second = crate::AuthorityReleaseValue::from(release.clone());
        second.release_id =
            AuthorityReleaseId::parse("time-release-00000000-0000-7000-8000-000000000002").unwrap();
        second.source_digest_sha256 = "b".repeat(64).parse().unwrap();
        let second = catalog
            .create_release(&owner, second.build().unwrap())
            .await
            .unwrap();
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
        assert_eq!(retired.record_version.get(), 3);
        for (field, value) in [
            ("releaseId", json!(second.release_id)),
            (
                "sourceId",
                json!("time-source-00000000-0000-7000-8000-000000000002"),
            ),
            ("datasetKind", json!("tzdb")),
            ("sourceDigestSha256", json!("c".repeat(64))),
            ("versionLabel", json!("other")),
            ("artifactPath", json!("/other")),
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
                    acquisition_id: TimeAcquisitionId::parse(
                        "time-acquisition-00000000-0000-7000-8000-000000000001",
                    )
                    .unwrap(),
                    source_id: source.source_id.clone(),
                    expected_source_digest_sha256: Some("a".repeat(64).parse().unwrap()),
                    status: TimeAcquisitionStatus::Succeeded,
                    phase: crate::TimeAcquisitionPhase::Complete,
                    staged_release_id: Some(release.release_id.clone()),
                    message: "".into(),
                    created_at: now,
                    updated_at: now,
                    record_version: crate::TimeVersion::new(1).unwrap(),
                },
                "acquire".into(),
            )
            .await
            .unwrap();
        let record = RecordId::new("time_acquisition", acquisition.acquisition_id.to_string());
        let original = body(&db.a, &record).await;
        for invalid in ["unknown", "staged", ""] {
            let rejected =
                db.a.client()
                    .query(include_str!("../../tests/queries/merge_record.surql"))
                    .bind(("record", record.clone()))
                    .bind((
                        "patch",
                        BTreeMap::from([("phase".to_owned(), invalid.into_value())]),
                    ))
                    .await
                    .unwrap()
                    .check();
            assert!(
                rejected.is_err(),
                "unknown acquisition phase must reject atomically"
            );
            assert_eq!(
                catalog
                    .acquisition(&owner, &acquisition.acquisition_id)
                    .await
                    .unwrap()
                    .unwrap(),
                acquisition
            );
        }

        for (field, value) in [
            (
                "acquisitionId",
                json!("time-acquisition-00000000-0000-7000-8000-000000000002"),
            ),
            (
                "sourceId",
                json!("time-source-00000000-0000-7000-8000-000000000002"),
            ),
            ("expectedSourceDigestSha256", json!("b".repeat(64))),
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
            TimeSourceId::parse("time-source-00000000-0000-7000-8000-000000000002").unwrap();
        let mut wrong_digest = acquisition.clone();
        wrong_digest.expected_source_digest_sha256 = None;
        let mut wrong_creation = acquisition.clone();
        wrong_creation.created_at -= chrono::Duration::seconds(1);
        for wrong in [wrong_source, wrong_digest, wrong_creation] {
            veoveo_types::Check::check(&wrong).unwrap();
            assert!(catalog.update_acquisition(&owner, wrong).await.is_err());
            assert_eq!(
                body(&db.a, &record).await,
                original,
                "invalid updates must not mutate storage"
            );
        }
        let mut updated = acquisition.clone();
        updated.status = TimeAcquisitionStatus::Succeeded;
        updated.phase = crate::TimeAcquisitionPhase::Complete;
        let updated = catalog.update_acquisition(&owner, updated).await.unwrap();
        assert_eq!(updated.record_version.get(), 2);
        let other = TimeCatalog::new(db.a.clone());
        let mut left = updated.clone();
        left.message = "left concurrent update".into();
        let mut right = updated;
        right.message = "right concurrent update".into();
        let (left, right) = tokio::join!(
            catalog.update_acquisition(&owner, left),
            other.update_acquisition(&owner, right),
        );
        assert_ne!(left.is_ok(), right.is_ok(), "one version fence may advance");
        let winner = left.or(right).unwrap();
        assert_eq!(winner.record_version.get(), 3);
        assert!(matches!(
            winner.message.as_str(),
            "left concurrent update" | "right concurrent update"
        ));
        assert_eq!(
            catalog
                .acquisition(&owner, &acquisition.acquisition_id)
                .await
                .unwrap()
                .unwrap(),
            winner
        );
        // Mutable lifecycle columns supersede an earlier admitted queued body.
        let queued_body = corrupt(&original, "status", json!("queued"));
        let queued_body = corrupt(&queued_body, "phase", json!("queued"));
        let queued_body = corrupt(&queued_body, "staged_release_id", serde_json::Value::Null);
        let queued = serde_json::from_str::<TimeAcquisition>(&queued_body).unwrap();
        set(&db.a, &record, "canonical_json", queued_body.clone()).await;
        let restored = catalog
            .acquisition(&owner, &acquisition.acquisition_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(restored.status, TimeAcquisitionStatus::Succeeded);
        assert_eq!(restored.phase, winner.phase);
        assert_eq!(restored.staged_release_id, winner.staged_release_id);
        assert_eq!(restored.updated_at, winner.updated_at);
        // Display metadata belongs to canonical_json, unlike the native lifecycle columns.
        assert_eq!(restored.message, queued.message);
        assert_eq!(restored.record_version.get(), 3);
        assert_eq!(
            body(&db.a, &record).await,
            queued_body,
            "reading must not rewrite the stale body"
        );
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

#[tokio::test]
async fn matching_subsecond_corruption_rejects_reads_without_rewriting_rows() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let owner = scope(&db.a, "time-subseconds", "owner").await;
        let foreign = scope(&db.a, "time-subseconds-other", "owner").await;
        let catalog = TimeCatalog::new(db.b.clone());
        let mut value = instant();
        value.nanosecond = SubsecondNanoseconds::MAX;
        let epoch = catalog
            .create_epoch(
                &owner,
                MissionEpoch {
                    epoch_id: MissionEpochId::parse("epoch-00000000-0000-7000-8000-000000000001")
                        .unwrap(),
                    name: "epoch".into(),
                    instant: value.clone(),
                    version: TimeVersion::FIRST,
                },
            )
            .await
            .unwrap();
        let event = catalog
            .create_event(
                &owner,
                TemporalEvent {
                    event_id: TemporalEventId::parse("event-00000000-0000-7000-8000-000000000001")
                        .unwrap(),
                    name: "event".into(),
                    due: value,
                    state: TemporalEventState::Scheduled,
                    record_version: TimeVersion::FIRST,
                },
                "subseconds".into(),
            )
            .await
            .unwrap();
        let records = [
            (
                RecordId::new("time_mission_epoch", format!("{}:1", epoch.epoch_id)),
                "instant",
                "nanosecond",
            ),
            (
                RecordId::new("time_temporal_event", event.event_id.to_string()),
                "due",
                "due_nanosecond",
            ),
        ];
        let mut originals = Vec::new();
        for (record, _, _) in &records {
            originals.push(body(&db.a, record).await);
        }
        for nanos in [-1_i64, 1_000_000_000, i64::from(u32::MAX), i64::MAX] {
            for ((record, field, column), original) in records.iter().zip(&originals) {
                let mut corrupted: serde_json::Value = serde_json::from_str(original).unwrap();
                corrupted[field]["nanosecond"] = nanos.into();
                set(&db.a, record, "canonical_json", corrupted.to_string()).await;
                set(&db.a, record, column, nanos).await;
            }
            let errors = [
                catalog.epoch(&owner, &epoch.epoch_id).await.unwrap_err(),
                catalog.epochs_page(&owner, None).await.unwrap_err(),
                catalog.event(&owner, &event.event_id).await.unwrap_err(),
                catalog.events_page(&owner, None, None).await.unwrap_err(),
            ];
            for error in errors {
                let message = format!("{error:#}");
                assert!(message.contains("canonical_json"));
                assert!(!message.contains(&nanos.to_string()));
            }
            assert!(
                catalog
                    .epochs_page(&foreign, None)
                    .await
                    .unwrap()
                    .items
                    .is_empty()
            );
            assert!(
                catalog
                    .events_page(&foreign, None, None)
                    .await
                    .unwrap()
                    .items
                    .is_empty()
            );
            for ((record, field, _), original) in records.iter().zip(&originals) {
                let mut expected: serde_json::Value = serde_json::from_str(original).unwrap();
                expected[field]["nanosecond"] = nanos.into();
                assert_eq!(body(&db.a, record).await, expected.to_string());
            }
        }
        for ((record, _, column), original) in records.iter().zip(&originals) {
            set(&db.a, record, "canonical_json", original.clone()).await;
            set(
                &db.a,
                record,
                column,
                i64::from(SubsecondNanoseconds::MAX.get()),
            )
            .await;
        }
        assert_eq!(
            catalog.epoch(&owner, &epoch.epoch_id).await.unwrap(),
            Some(epoch)
        );
        assert_eq!(
            catalog.event(&owner, &event.event_id).await.unwrap(),
            Some(event)
        );
    })
    .await
    .expect("Time subsecond metadata qualification exceeded 90 seconds");
}

#[tokio::test]
async fn retained_instants_reject_ambiguous_authority_bindings_after_sql_visibility() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let owner = scope(&db.a, "time-binding", "owner").await;
        let foreign = scope(&db.a, "time-binding-other", "owner").await;
        let catalog = TimeCatalog::new(db.b.clone());
        let epoch = catalog
            .create_epoch(
                &owner,
                MissionEpoch {
                    epoch_id: MissionEpochId::parse("epoch-00000000-0000-7000-8000-000000000001")
                        .unwrap(),
                    name: "epoch".into(),
                    instant: instant(),
                    version: TimeVersion::FIRST,
                },
            )
            .await
            .unwrap();
        let event = super::event(
            &catalog,
            &owner,
            "event-00000000-0000-7000-8000-000000000001",
        )
        .await;
        let records = [
            (
                RecordId::new("time_mission_epoch", format!("{}:1", epoch.epoch_id)),
                "instant",
            ),
            (
                RecordId::new("time_temporal_event", event.event_id.to_string()),
                "due",
            ),
        ];
        let mut originals = Vec::new();
        let mut corrupted_bodies = Vec::new();
        for (record, field) in &records {
            let original = body(&db.a, record).await;
            let mut corrupted: serde_json::Value = serde_json::from_str(&original).unwrap();
            corrupted[field]["authority"]["leapSecondsReleaseId"] =
                corrupted[field]["authority"]["tzdbReleaseId"].clone();
            let corrupted = corrupted.to_string();
            set(&db.a, record, "canonical_json", corrupted.clone()).await;
            originals.push(original);
            corrupted_bodies.push(corrupted);
        }
        for error in [
            catalog.epoch(&owner, &epoch.epoch_id).await.unwrap_err(),
            catalog.epochs_page(&owner, None).await.unwrap_err(),
            catalog.event(&owner, &event.event_id).await.unwrap_err(),
            catalog.events_page(&owner, None, None).await.unwrap_err(),
        ] {
            assert!(error.to_string().contains("canonical_json"));
        }
        assert!(
            catalog
                .epoch(&foreign, &epoch.epoch_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            catalog
                .event(&foreign, &event.event_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            catalog
                .epochs_page(&foreign, None)
                .await
                .unwrap()
                .items
                .is_empty()
        );
        assert!(
            catalog
                .events_page(&foreign, None, None)
                .await
                .unwrap()
                .items
                .is_empty()
        );
        for (((record, _), original), corrupted) in
            records.iter().zip(originals).zip(corrupted_bodies)
        {
            assert_eq!(body(&db.a, record).await, corrupted);
            set(&db.a, record, "canonical_json", original).await;
        }
        assert_eq!(
            catalog.epoch(&owner, &epoch.epoch_id).await.unwrap(),
            Some(epoch)
        );
        assert_eq!(
            catalog.event(&owner, &event.event_id).await.unwrap(),
            Some(event)
        );
    })
    .await
    .expect("Time authority binding metadata qualification exceeded 90 seconds");
}
