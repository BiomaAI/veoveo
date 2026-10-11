//! Actual Time watchers settle durable events and invalidate their subscribed root.
use super::{hosted::Fixture, *};
use veoveo_mcp_contract::ResourceUpdate;

#[tokio::test]
async fn future_cancelled_and_overdue_events_use_real_scheduler_and_owner_recovery() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let fixture = Fixture::new().await;
        let state = &fixture.state;
        let owner = scope(&fixture.db.a, "event-owner").await;
        let engine = state.engine(&owner).await.unwrap();
        let now = engine
            .resolve(&ResolveTimeRequest {
                expression: TimeExpressionValue::Rfc3339 {
                    value: Utc::now().to_rfc3339(),
                }
                .build()
                .unwrap(),
                additional_uncertainty_nanoseconds: 0,
            })
            .unwrap()
            .into_instant();
        let mut updates = state.subscriptions.listen();
        let mut events = Vec::new();
        for (index, offset) in [1_000_000_000i128, 1_500_000_000, -1_000_000_000]
            .into_iter()
            .enumerate()
        {
            events.push(
                state
                    .catalog
                    .create_event(
                        &owner,
                        TemporalEvent {
                            event_id: TemporalEventId::parse(format!("event-{}", Uuid::now_v7()))
                                .unwrap(),
                            name: format!("scheduler native {index}"),
                            due: TimeInstant::from_total_nanoseconds(
                                now.total_nanoseconds() + offset,
                                0,
                                now.authority.clone(),
                            )
                            .unwrap(),
                            state: TemporalEventState::Scheduled,
                            record_version: TimeVersion::FIRST,
                        },
                        format!("native-{index}"),
                    )
                    .await
                    .unwrap(),
            );
        }
        let mut replay = events[0].clone();
        replay.event_id = TemporalEventId::parse(format!("event-{}", Uuid::now_v7())).unwrap();
        assert_eq!(
            state
                .catalog
                .create_event(&owner, replay.clone(), "native-0".into())
                .await
                .unwrap(),
            events[0]
        );
        replay.name.push_str(" conflicting payload");
        assert!(
            state
                .catalog
                .create_event(&owner, replay, "native-0".into())
                .await
                .is_err()
        );
        state
            .schedule_event(owner.clone(), events[0].clone())
            .await
            .unwrap();
        state
            .schedule_event(owner.clone(), events[1].clone())
            .await
            .unwrap();
        let cancelled = state
            .catalog
            .cancel_event(&owner, &events[1].event_id, events[1].record_version)
            .await
            .unwrap();
        state
            .cancel_event_watcher(&owner, &events[1].event_id)
            .await;
        assert!(
            state
                .catalog
                .cancel_event(&owner, &events[1].event_id, events[1].record_version)
                .await
                .is_err()
        );
        // An overdue persisted event has no local watcher: the same owner root
        // restore path used by subscriptions must recover it.
        state.restore_event_watchers(&owner).await.unwrap();
        let mut due_ids = std::collections::BTreeSet::new();
        while due_ids.len() != 2 {
            let update = updates.recv().await.unwrap();
            if let ResourceUpdate::Uri(uri) = update {
                if uri == crate::uris::EVENTS_URI {
                    for event in [&events[0], &events[2]] {
                        let current = state
                            .catalog
                            .event(&owner, &event.event_id)
                            .await
                            .unwrap()
                            .unwrap();
                        if current.state == TemporalEventState::Due {
                            assert_eq!(current.name, event.name);
                            assert_eq!(current.due, event.due);
                            assert_eq!(
                                current.record_version,
                                event.record_version.checked_next().unwrap()
                            );
                            due_ids.insert(current.event_id.to_string());
                        }
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(1550)).await;
        assert_eq!(
            state
                .catalog
                .event(&owner, &cancelled.event_id)
                .await
                .unwrap()
                .unwrap(),
            cancelled
        );
        let foreign = scope(&fixture.db.a, "another-event-owner").await;
        assert!(
            state
                .catalog
                .event(&foreign, &events[0].event_id)
                .await
                .unwrap()
                .is_none()
        );
        for event in events {
            state.cancel_event_watcher(&owner, &event.event_id).await;
        }
    })
    .await
    .expect("native temporal event qualification exceeded thirty seconds");
}
