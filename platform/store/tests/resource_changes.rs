//! Native reconnect qualification in an owned pinned SurrealDB fixture.
use std::time::Duration;

use futures::StreamExt;
use veoveo_modules::TableName;
use veoveo_platform_store::{ObservationReplay, ObservationTable, ResourceInvalidation};
#[path = "../../../testing/fixtures/connection_switch.rs"]
mod connection_switch;
#[path = "../../../testing/fixtures/store.rs"]
mod store;
use connection_switch::ConnectionSwitch;

async fn task_live_ids(writer: &veoveo_platform_store::PlatformStore) -> Vec<String> {
    let mut response = writer
        .client()
        .query(include_str!("queries/resource_changes/task_live_ids.surql"))
        .await
        .unwrap()
        .check()
        .unwrap();
    let info: Option<serde_json::Value> = response.take(0).unwrap();
    info.unwrap()["lives"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

#[tokio::test]
async fn replica_changes_and_reconnect_invalidate_without_outbox_or_idle_polling() {
    let fixture = store::TestDb::new().await;
    let writer = fixture.a.clone();
    let endpoint = writer.config().endpoint().to_string();
    let test_writer = writer.clone();
    let mut test = Box::pin(async {
        // Deliberately define no outbox. The same table identities used by the
        // Frames, Media and Recording hubs must deliver directly from the store.
        test_writer
            .client()
            .query(
                include_str!("queries/resource_changes/replica_changes_and_reconnect_invalidate_without_outbox_or_idle_polling.surql"),
            )
            .await
            .unwrap()
            .check()
            .unwrap();
        let remote = url::Url::parse(&endpoint).unwrap();
        assert_eq!(
            remote.scheme(),
            "ws",
            "qualification requires a ws endpoint"
        );
        let switch = ConnectionSwitch::start(
            remote.host_str().unwrap().to_owned(),
            remote.port_or_known_default().unwrap(),
        )
        .await;
        let reader = fixture.connect_at(&switch.endpoint).await;
        let mut changes = reader.resource_changes(
            [
                "observation_fixture_frame_world",
                "observation_fixture_provider_job",
                "observation_fixture_media_usage",
                "observation_fixture_recording",
                "observation_fixture_task",
                "observation_fixture_domain_usage",
            ]
            .into_iter()
            .map(|name| {
                ObservationTable::new(TableName::new(name).unwrap(), ObservationReplay::LiveOnly)
            })
            .collect(),
        );
        assert_eq!(
            changes.next().await,
            Some(ResourceInvalidation::Reconcile),
            "initial reconciliation"
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(250), changes.next())
                .await
                .is_err(),
            "an idle source must not wake"
        );
        for statement in [
            include_str!(
                "queries/resource_changes/replica_changes_and_reconnect_invalidate_without_outbox_or_idle_polling_2.surql"
            ),
            include_str!(
                "queries/resource_changes/replica_changes_and_reconnect_invalidate_without_outbox_or_idle_polling_3.surql"
            ),
            include_str!(
                "queries/resource_changes/replica_changes_and_reconnect_invalidate_without_outbox_or_idle_polling_4.surql"
            ),
            include_str!(
                "queries/resource_changes/replica_changes_and_reconnect_invalidate_without_outbox_or_idle_polling_5.surql"
            ),
            include_str!(
                "queries/resource_changes/replica_changes_and_reconnect_invalidate_without_outbox_or_idle_polling_6.surql"
            ),
            include_str!(
                "queries/resource_changes/replica_changes_and_reconnect_invalidate_without_outbox_or_idle_polling_7.surql"
            ),
        ] {
            test_writer
                .client()
                .query(statement)
                .await
                .unwrap()
                .check()
                .unwrap();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(5), changes.next())
                    .await
                    .expect("replica update missing"),
                Some(ResourceInvalidation::Live)
            );
        }
        let initial_ids = task_live_ids(&writer).await;
        switch.set_enabled(false).await;
        // These commits occur while the observer cannot receive LIVE events.
        test_writer
            .client()
            .query(
                include_str!("queries/resource_changes/replica_changes_and_reconnect_invalidate_without_outbox_or_idle_polling_8.surql"),
            )
            .await
            .unwrap()
            .check()
            .unwrap();
        switch.set_enabled(true).await;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(15), changes.next())
                .await
                .expect("reconnect recovery missing"),
            Some(ResourceInvalidation::Changefeed)
        );
        test_writer
            .client()
            .query(include_str!("queries/resource_changes/replica_changes_and_reconnect_invalidate_without_outbox_or_idle_polling_9.surql"))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), changes.next())
                .await
                .expect("LIVE did not resume"),
            Some(ResourceInvalidation::Live)
        );
        let final_ids = task_live_ids(&writer).await;
        drop(changes);
        let mut retained = std::collections::BTreeMap::new();
        let cleanup = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let mut remaining = 0;
                for name in [
                    "frame_world",
                    "provider_job",
                    "media_usage",
                    "recording",
                    "task",
                    "domain_usage",
                ] {
                    let mut response = writer
                        .client()
                        .query(include_str!("queries/resource_changes/replica_changes_and_reconnect_invalidate_without_outbox_or_idle_polling_11.surql"))
            .bind(("table", format!("observation_fixture_{name}")))
                        .await
                        .unwrap()
                        .check()
                        .unwrap();
                    let info: Option<serde_json::Value> = response.take(0).unwrap();
                    let count = info.unwrap()["lives"]
                        .as_object()
                        .expect("native LIVE catalog")
                        .len();
                    retained.insert(name, count);
                    remaining += count;
                }
                if remaining == 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await;
        assert!(
            cleanup.is_ok(),
            "dropped observation retained native LIVE queries: {retained:?}; initial={initial_ids:?} final={final_ids:?} after={:?}",
            task_live_ids(&writer).await
        );
    });
    let result = tokio::time::timeout(Duration::from_secs(60), &mut test).await;
    result.expect("resource qualification exceeded 60 seconds");
}
