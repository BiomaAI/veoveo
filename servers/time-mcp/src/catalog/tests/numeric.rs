use super::{instant, metadata::set, scope};
use crate::{
    catalog::TimeCatalog, contract::*, persistence::TimeClockPolicyRecord, test_store::TestDb,
};
use std::time::Duration;
use surrealdb::types::RecordId;
use veoveo_platform_store::PlatformStore;

fn policy(stratum: u8) -> ClockQualityPolicy {
    ClockQualityPolicy::builder()
        .maximum_error_nanoseconds(1000)
        .maximum_stratum(stratum)
        .minimum_source_diversity(2)
        .maximum_holdover_seconds(300)
        .build()
        .unwrap()
}

async fn clock_row(store: &PlatformStore, record: &RecordId) -> TimeClockPolicyRecord {
    store
        .client()
        .select::<Option<TimeClockPolicyRecord>>(record.clone())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn clock_policy_decoding_rejects_bad_scalars_and_versions_without_mutation() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let owner = scope(&db.a, "time-clock-numeric", "owner").await;
        let foreign = scope(&db.a, "time-clock-numeric-other", "owner").await;
        let catalog = TimeCatalog::new(db.b.clone());
        let (created, version) = catalog
            .replace_clock_policy(&owner, policy(4), TimeWriteGuard::Absent)
            .await
            .unwrap();
        assert_eq!(created, policy(4));
        assert_eq!(version, TimeVersion::FIRST);
        assert!(
            catalog
                .replace_clock_policy(&owner, policy(3), TimeWriteGuard::Absent)
                .await
                .is_err()
        );
        let record = RecordId::new("time_clock_policy", owner.identity.tenant_id.to_string());
        for (field, bad, restore) in [
            ("maximum_error_nanoseconds", -1, 1000),
            ("maximum_error_nanoseconds", 0, 1000),
            ("maximum_stratum", -1, 4),
            ("maximum_stratum", 0, 4),
            ("maximum_stratum", 16, 4),
            ("maximum_stratum", 256, 4),
            ("minimum_source_diversity", -1, 2),
            ("minimum_source_diversity", 0, 2),
            ("minimum_source_diversity", u32::MAX as i64 + 1, 2),
            ("maximum_holdover_seconds", -1, 300),
            ("maximum_holdover_seconds", 0, 300),
            ("record_version", -1, 1),
            ("record_version", 0, 1),
        ] {
            set(&db.a, &record, field, bad).await;
            let retained = clock_row(&db.a, &record).await;
            let error = catalog.clock_policy(&owner).await.unwrap_err();
            assert!(error.to_string().contains(field), "{error}");
            assert!(catalog.clock_policy(&foreign).await.unwrap().is_none());
            assert_eq!(clock_row(&db.a, &record).await, retained);
            set(&db.a, &record, field, restore).await;
        }
        let other = TimeCatalog::new(db.a.clone());
        let (left, right) = tokio::join!(
            catalog.replace_clock_policy(
                &owner,
                policy(3),
                TimeWriteGuard::Existing(TimeVersion::FIRST)
            ),
            other.replace_clock_policy(
                &owner,
                policy(5),
                TimeWriteGuard::Existing(TimeVersion::FIRST)
            ),
        );
        assert_ne!(left.is_ok(), right.is_ok());
        let winner = left.or(right).unwrap();
        assert_eq!(winner.1.get(), 2);
        assert_eq!(catalog.clock_policy(&owner).await.unwrap().unwrap(), winner);
        set(&db.a, &record, "record_version", i64::MAX).await;
        let retained = clock_row(&db.a, &record).await;
        let (_, max) = catalog.clock_policy(&owner).await.unwrap().unwrap();
        assert_eq!(max.get(), i64::MAX as u64);
        assert!(
            catalog
                .replace_clock_policy(&owner, policy(6), TimeWriteGuard::Existing(max))
                .await
                .is_err()
        );
        assert_eq!(clock_row(&db.a, &record).await, retained);
    })
    .await
    .expect("Time clock numeric qualification exceeded 90 seconds");
}

#[tokio::test]
async fn exhausted_source_acquisition_and_event_versions_cannot_advance() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let owner = scope(&db.a, "time-version-numeric", "owner").await;
        let catalog = TimeCatalog::new(db.b.clone());
        let max = TimeVersion::new(i64::MAX as u64).unwrap();
        let source = catalog
            .create_source(
                &owner,
                crate::NewTimeSource {
                    source_id: TimeSourceId::parse(
                        "time-source-00000000-0000-7000-8000-000000000001",
                    )
                    .unwrap(),
                    name: "source".into(),
                    dataset_kind: AuthorityDatasetKind::Tzdb,
                    url: "https://example.test/tzdb".into(),
                    expected_content_type: "application/gzip".into(),
                    enabled: true,
                    record_version: crate::SourceCreationVersion,
                },
            )
            .await
            .unwrap();
        let source_record = RecordId::new("time_source", source.source_id.to_string());
        set(&db.a, &source_record, "record_version", i64::MAX).await;
        let retained = catalog
            .source(&owner, &source.source_id)
            .await
            .unwrap()
            .unwrap();
        let mut changed = retained.clone();
        changed.enabled = false;
        assert!(catalog.replace_source(&owner, changed, max).await.is_err());
        assert_eq!(
            catalog
                .source(&owner, &source.source_id)
                .await
                .unwrap()
                .unwrap(),
            retained
        );

        let now = chrono::Utc::now();
        let acquisition = catalog
            .create_acquisition(
                &owner,
                TimeAcquisition {
                    acquisition_id: TimeAcquisitionId::parse(
                        "time-acquisition-00000000-0000-7000-8000-000000000001",
                    )
                    .unwrap(),
                    source_id: source.source_id,
                    expected_source_digest_sha256: None,
                    status: TimeAcquisitionStatus::Queued,
                    phase: "queued".into(),
                    staged_release_id: None,
                    message: "".into(),
                    created_at: now,
                    updated_at: now,
                    record_version: crate::TimeVersion::new(1).unwrap(),
                },
                "numeric".into(),
            )
            .await
            .unwrap();
        let record = RecordId::new("time_acquisition", acquisition.acquisition_id.to_string());
        set(&db.a, &record, "record_version", i64::MAX).await;
        let retained = catalog
            .acquisition(&owner, &acquisition.acquisition_id)
            .await
            .unwrap()
            .unwrap();
        for version in [0, i64::MAX as u64, u64::MAX] {
            let mut wire = serde_json::to_value(&retained).unwrap();
            wire["record_version"] = version.into();
            wire["phase"] = "changed".into();
            match serde_json::from_value::<TimeAcquisition>(wire) {
                Ok(changed) => assert!(catalog.update_acquisition(&owner, changed).await.is_err()),
                Err(_) => assert!(crate::TimeVersion::new(version).is_err()),
            }
            assert_eq!(
                catalog
                    .acquisition(&owner, &acquisition.acquisition_id)
                    .await
                    .unwrap()
                    .unwrap(),
                retained
            );
        }
        let event = catalog
            .create_event(
                &owner,
                TemporalEvent {
                    event_id: TemporalEventId::parse("event-00000000-0000-7000-8000-000000000001")
                        .unwrap(),
                    name: "event".into(),
                    due: instant(),
                    state: TemporalEventState::Scheduled,
                    record_version: crate::TimeVersion::new(1).unwrap(),
                },
                "numeric".into(),
            )
            .await
            .unwrap();
        let record = RecordId::new("time_temporal_event", event.event_id.to_string());
        set(&db.a, &record, "record_version", i64::MAX).await;
        let retained = catalog
            .event(&owner, &event.event_id)
            .await
            .unwrap()
            .unwrap();
        assert!(
            catalog
                .cancel_event(&owner, &event.event_id, max)
                .await
                .is_err()
        );
        assert!(
            catalog
                .mark_event_due(&owner, &event.event_id, max)
                .await
                .is_err()
        );
        assert_eq!(
            catalog
                .event(&owner, &event.event_id)
                .await
                .unwrap()
                .unwrap(),
            retained
        );
    })
    .await
    .expect("Time version numeric qualification exceeded 90 seconds");
}
