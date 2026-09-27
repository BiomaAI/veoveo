use std::time::Duration;

use veoveo_platform_store::{
    PlatformStore, PrincipalKind, TimeCompletion, TimeTemporalEventState as StoredEventState,
};
use veoveo_time_mcp::{
    AuthorityBinding, AuthorityReleaseId, CalendarId, MissionEpoch, MissionEpochId,
    OperationalCalendar, TemporalEvent, TemporalEventId, TemporalEventState, TimeInstant,
    catalog::{TimeCatalog, TimeScope},
};

#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

async fn scope(store: &PlatformStore, tenant: &str, owner: &str) -> TimeScope {
    TimeScope {
        identity: store
            .ensure_identity(
                tenant,
                owner,
                "https://time.example",
                owner,
                PrincipalKind::User,
            )
            .await
            .unwrap(),
    }
}

fn instant() -> TimeInstant {
    TimeInstant {
        tai_seconds_since_1970: 2_000_000_000,
        nanosecond: 17,
        uncertainty_nanoseconds: 0,
        authority: AuthorityBinding {
            tzdb_release_id: AuthorityReleaseId::new("time-release-tzdb").unwrap(),
            leap_seconds_release_id: AuthorityReleaseId::new("time-release-leaps").unwrap(),
        },
    }
}

async fn event(catalog: &TimeCatalog, owner: &TimeScope, key: &str) -> TemporalEvent {
    catalog
        .create_event(
            owner,
            TemporalEvent {
                event_id: TemporalEventId::new(key).unwrap(),
                name: key.into(),
                due: instant(),
                state: TemporalEventState::Scheduled,
                record_version: 1,
            },
            key.into(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn sql_filters_owner_tenant_latest_version_and_completion_before_limit() {
    // This is native Store qualification; no reference installation or GPU is required.
    let db = fixture::TestDb::new().await;
    tokio::time::timeout(Duration::from_secs(90), async {
        let owner = scope(&db.a, "time-query-test", "owner").await;
        let peer = scope(&db.a, "time-query-test", "peer").await;
        let foreign = scope(&db.a, "time-query-other", "owner").await;
        let catalog = TimeCatalog::new(db.b.clone());

        for i in 0..102 {
            event(
                &catalog,
                &peer,
                &format!("event-00000000-0000-7000-8000-{i:012x}"),
            )
            .await;
            event(
                &catalog,
                &owner,
                &format!("event-ffffffff-ffff-7000-8000-{i:012x}"),
            )
            .await;
        }
        let foreign_event = event(
            &catalog,
            &foreign,
            "event-00000000-0000-7000-9000-000000000000",
        )
        .await;
        let peer_event =
            TemporalEventId::new("event-00000000-0000-7000-8000-000000000000").unwrap();
        assert!(catalog.event(&owner, &peer_event).await.unwrap().is_none());
        assert!(
            catalog
                .event(&owner, &foreign_event.event_id)
                .await
                .unwrap()
                .is_none()
        );
        let own =
            db.a.list_time_temporal_events(&owner.identity)
                .await
                .unwrap();
        assert_eq!(own.len(), 102);
        assert!(
            own.iter()
                .all(|row| row.owner == owner.identity.principal_id.record_id())
        );
        assert_eq!(catalog.list_events(&owner).await.unwrap().len(), 102);
        let due =
            db.a.due_time_temporal_events(&owner.identity, 2_000_000_000, 17, 3)
                .await
                .unwrap();
        assert_eq!(due.len(), 3);
        assert!(
            due.iter()
                .all(|row| row.owner == owner.identity.principal_id.record_id())
        );

        // A Store caller cannot bypass the same owner predicate on a transition.
        assert!(
            db.a.transition_time_temporal_event(
                &owner.identity,
                peer_event.as_str(),
                1,
                StoredEventState::Cancelled,
                "{}".into(),
            )
            .await
            .is_err()
        );
        assert_eq!(
            catalog
                .event(&peer, &peer_event)
                .await
                .unwrap()
                .unwrap()
                .state,
            TemporalEventState::Scheduled
        );
        let own_id = TemporalEventId::new("event-ffffffff-ffff-7000-8000-000000000000").unwrap();
        assert_eq!(
            catalog
                .cancel_event(&owner, &own_id, 1)
                .await
                .unwrap()
                .state,
            TemporalEventState::Cancelled
        );
        assert!(catalog.cancel_event(&owner, &own_id, 1).await.is_err());

        let matches =
            db.a.complete_time_values(&owner.identity, TimeCompletion::EventId, "", 101)
                .await
                .unwrap();
        assert_eq!(matches.len(), 101);
        assert_eq!(
            matches.first().unwrap(),
            "event-ffffffff-ffff-7000-8000-000000000000"
        );
        assert_eq!(
            matches.last().unwrap(),
            "event-ffffffff-ffff-7000-8000-000000000064"
        );
        assert_eq!(
            db.a.complete_time_values(
                &owner.identity,
                TimeCompletion::EventId,
                "FFFFFFFF-FFFF-7000-8000-000000000065",
                101
            )
            .await
            .unwrap(),
            vec!["event-ffffffff-ffff-7000-8000-000000000065"]
        );
        assert!(
            db.a.complete_time_values(
                &owner.identity,
                TimeCompletion::EventId,
                "00000000-0000-7000",
                101
            )
            .await
            .unwrap()
            .is_empty()
        );
        assert!(
            db.a.complete_time_values(&owner.identity, TimeCompletion::EventId, "' OR true;", 101)
                .await
                .unwrap()
                .is_empty()
        );
        for limit in [0, 102] {
            assert!(
                db.a.complete_time_values(&owner.identity, TimeCompletion::EventId, "", limit)
                    .await
                    .is_err()
            );
        }

        for version in [2, 12, 1] {
            catalog
                .create_epoch(
                    &owner,
                    MissionEpoch {
                        epoch_id: MissionEpochId::new("epoch-00000000-0000-7000-8000-000000000001")
                            .unwrap(),
                        name: "launch".into(),
                        instant: instant(),
                        version,
                    },
                )
                .await
                .unwrap();
            catalog
                .create_calendar(
                    &owner,
                    OperationalCalendar {
                        calendar_id: CalendarId::new(
                            "calendar-00000000-0000-7000-8000-000000000001",
                        )
                        .unwrap(),
                        version,
                        name: "Operations".into(),
                        zone_id: "UTC".into(),
                        windows: Vec::new(),
                        excluded_dates: Vec::new(),
                    },
                )
                .await
                .unwrap();
        }
        catalog
            .create_calendar(
                &owner,
                OperationalCalendar {
                    calendar_id: CalendarId::new("calendar-00000000-0000-7000-8000-000000000002")
                        .unwrap(),
                    version: 42,
                    name: "Other".into(),
                    zone_id: "UTC".into(),
                    windows: Vec::new(),
                    excluded_dates: Vec::new(),
                },
            )
            .await
            .unwrap();
        let epoch_id = MissionEpochId::new("epoch-00000000-0000-7000-8000-000000000001").unwrap();
        assert_eq!(
            catalog
                .epoch(&owner, &epoch_id)
                .await
                .unwrap()
                .unwrap()
                .version,
            12
        );
        assert_eq!(
            catalog
                .epoch(&peer, &epoch_id)
                .await
                .unwrap()
                .unwrap()
                .version,
            12
        );
        assert!(catalog.epoch(&foreign, &epoch_id).await.unwrap().is_none());
        assert!(
            catalog
                .epoch(
                    &owner,
                    &MissionEpochId::new("epoch-00000000-0000-7000-8000-000000000002").unwrap()
                )
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            db.a.complete_time_values(
                &owner.identity,
                TimeCompletion::EpochId,
                "EPOCH-00000000-0000-7000-8000-000000000001",
                101
            )
            .await
            .unwrap(),
            vec!["epoch-00000000-0000-7000-8000-000000000001"]
        );
        assert_eq!(
            db.a.complete_time_values(
                &owner.identity,
                TimeCompletion::CalendarId,
                "CALENDAR-00000000-0000-7000-8000-000000000001",
                101
            )
            .await
            .unwrap(),
            vec!["calendar-00000000-0000-7000-8000-000000000001"]
        );
        let versions = || TimeCompletion::CalendarVersion {
            calendar_key: Some("calendar-00000000-0000-7000-8000-000000000001".into()),
        };
        assert_eq!(
            db.a.complete_time_values(&peer.identity, versions(), "", 101)
                .await
                .unwrap(),
            vec!["1", "12", "2"]
        );
        assert!(
            db.a.complete_time_values(&peer.identity, versions(), "42", 101)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            db.a.complete_time_values(&foreign.identity, versions(), "", 101)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            db.a.complete_time_values(
                &owner.identity,
                TimeCompletion::CalendarVersion { calendar_key: None },
                "42",
                101
            )
            .await
            .unwrap(),
            vec!["42"]
        );
    })
    .await
    .expect("Time Store qualification exceeded 90 seconds");
}
