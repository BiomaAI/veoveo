use super::*;
use crate::{TimeResource, TimeResourceEntry, TimeResourcePage, TimeVersion};
use veoveo_mcp_knowledge_extension::{ModifiedBy, ReadPolicy};
use veoveo_types::{AccessSubject, ResourceAddress};

#[tokio::test]
async fn observations_use_stored_authority_and_versioned_members() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let catalog = TimeCatalog::new(db.b.clone());
        let owner = scope(&db.a, "time-knowledge", "creator").await;
        let mut peer = scope(&db.a, "time-knowledge", "reader").await;
        peer.work_context = "reader-context".parse().unwrap();
        let foreign = scope(&db.a, "foreign", "creator").await;
        let calendar = crate::OperationalCalendarValue {
            calendar_id: CalendarId::parse("calendar-00000000-0000-7000-8000-000000000010")
                .unwrap(),
            version: TimeVersion::FIRST,
            name: "Calendar".into(),
            zone_id: "UTC".into(),
            windows: vec![],
            excluded_dates: vec![],
        }
        .build()
        .unwrap();
        catalog
            .create_calendar(&owner, calendar.clone())
            .await
            .unwrap();
        let observed = catalog
            .observed_calendar(&peer, &calendar.calendar_id, calendar.version)
            .await
            .unwrap()
            .unwrap();
        let (text, observation) = observed.document().unwrap();
        assert_eq!(
            serde_json::from_str::<OperationalCalendar>(&text).unwrap(),
            calendar
        );
        let access = observation.access().unwrap();
        assert_eq!(
            observation.content_sha256(),
            &veoveo_mcp_knowledge_extension::content_digest(&text)
        );
        let current: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(current["calendarId"], calendar.calendar_id.to_string());
        assert_eq!(current["zoneId"], "UTC");
        let mut retired = current.clone();
        let object = retired.as_object_mut().unwrap();
        let value = object.remove("calendarId").unwrap();
        object.insert("calendar_id".into(), value);
        let retired_text = serde_json::to_string(&retired).unwrap();
        assert!(serde_json::from_str::<OperationalCalendar>(&retired_text).is_err());
        assert_ne!(
            veoveo_mcp_knowledge_extension::content_digest(&retired_text),
            *observation.content_sha256()
        );

        assert_eq!(
            access.owner,
            AccessSubject::Principal("creator".parse().unwrap())
        );
        assert_eq!(access.work_context, owner.work_context);
        assert_ne!(access.work_context, peer.work_context);
        assert_eq!(access.read_policy, ReadPolicy::Tenant {});
        assert_eq!(
            observation.modified_by(),
            Some(&ModifiedBy::Principal("creator".parse().unwrap()))
        );
        assert!(observation.modified_at().is_some());
        let mut oversized = crate::OperationalCalendarValue::from(calendar.clone());
        oversized.version = TimeVersion::new(2).unwrap();
        oversized.excluded_dates = vec!["2026-10-01".into(); 6_000];
        assert!(
            catalog
                .create_calendar(&owner, oversized.build().unwrap())
                .await
                .is_err()
        );
        assert!(
            catalog
                .calendar(&owner, &calendar.calendar_id, TimeVersion::new(2).unwrap())
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            catalog
                .observed_calendar(&foreign, &calendar.calendar_id, calendar.version)
                .await
                .unwrap()
                .is_none()
        );

        let epoch_id = MissionEpochId::parse("epoch-00000000-0000-7000-8000-000000000010").unwrap();
        for version in [1, 2] {
            catalog
                .create_epoch(
                    &owner,
                    MissionEpoch {
                        epoch_id: epoch_id.clone(),
                        name: "Mission".into(),
                        version: TimeVersion::new(version).unwrap(),
                        instant: instant(),
                    },
                )
                .await
                .unwrap();
        }
        let epochs = catalog.epochs_page(&peer, None).await.unwrap();
        let page = TimeResourcePage::from_page(epochs, |epoch| {
            TimeResourceEntry::new(
                TimeResource::EpochVersion {
                    id: epoch.epoch_id,
                    version: epoch.version,
                },
                epoch.name,
            )
        });
        assert_eq!(page.items.len(), 2);
        assert_ne!(page.items[0].uri, page.items[1].uri);
        for entry in &page.items {
            let TimeResource::EpochVersion { id, version } =
                TimeResource::parse(entry.uri.as_str()).unwrap()
            else {
                panic!("versioned epoch required")
            };
            let observed = catalog
                .observed_epoch(&peer, &id, version)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(observed.value().version, version);
            assert_eq!(
                observed
                    .document()
                    .unwrap()
                    .1
                    .access()
                    .unwrap()
                    .work_context,
                owner.work_context
            );
        }
        assert_eq!(
            catalog
                .complete_values(
                    &peer,
                    TimeCompletion::EpochVersion {
                        epoch_key: Some(epoch_id)
                    },
                    "",
                    100
                )
                .await
                .unwrap(),
            ["1", "2"]
        );
        let wire = serde_json::to_value(&page).unwrap();
        assert!(wire.get("nextCursor").is_none());

        let event = event(
            &catalog,
            &owner,
            "event-00000000-0000-7000-8000-000000000010",
        )
        .await;
        let observed = catalog
            .observed_event(&owner, &event.event_id)
            .await
            .unwrap()
            .unwrap();
        let (_, first) = observed.document().unwrap();
        assert_eq!(first.access().unwrap().read_policy, ReadPolicy::Subjects {});
        assert!(
            catalog
                .observed_event(&peer, &event.event_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            catalog
                .observed_event(&foreign, &event.event_id)
                .await
                .unwrap()
                .is_none()
        );
        let mut selected_elsewhere = owner.clone();
        selected_elsewhere.work_context = "elsewhere".parse().unwrap();
        assert_eq!(
            catalog
                .observed_event(&selected_elsewhere, &event.event_id)
                .await
                .unwrap()
                .unwrap()
                .document()
                .unwrap()
                .1
                .revision(),
            first.revision()
        );
        catalog
            .cancel_event(&owner, &event.event_id, event.record_version)
            .await
            .unwrap();
        let updated = catalog
            .observed_event(&owner, &event.event_id)
            .await
            .unwrap()
            .unwrap();
        let (_, second) = updated.document().unwrap();
        assert_ne!(first.revision(), second.revision());
        assert_ne!(first.content_sha256(), second.content_sha256());
        assert!(second.modified_by().is_none());

        // Authorization happens in SQL even when the denied row cannot decode.
        db.a.client()
            .query(include_str!("../../tests/queries/corrupt_provenance.surql"))
            .bind((
                "record",
                surrealdb::types::RecordId::new("time_temporal_event", event.event_id.to_string()),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            catalog
                .observed_event(&peer, &event.event_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            catalog
                .observed_event(&owner, &event.event_id)
                .await
                .is_err()
        );
        assert!(
            TimeResource::EpochVersion {
                id: "epoch-example".parse().unwrap(),
                version: TimeVersion::FIRST
            }
            .to_uri()
            .is_ok()
        );
    })
    .await
    .expect("Time knowledge qualification exceeded 60 seconds");
}
