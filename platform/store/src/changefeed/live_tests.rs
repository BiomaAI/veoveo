use super::*;
use crate::ObservationReplay;
use veoveo_modules::TableName;
#[path = "../../../../testing/fixtures/store.rs"]
mod fixture;

#[derive(SurrealValue)]
struct Row {
    id: RecordId,
    value: i64,
}
fn table(name: &str) -> ObservationTable {
    ObservationTable::new(TableName::new(name).unwrap(), ObservationReplay::LiveOnly)
}
async fn count(db: &PlatformStore, table: &ObservationTable) -> usize {
    let mut response = db
        .client()
        .query(format!("INFO FOR TABLE {};", table.as_str()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let info: Option<serde_json::Value> = response.take(0).unwrap();
    info.unwrap()["lives"].as_object().unwrap().len()
}
async fn await_count(db: &PlatformStore, table: &ObservationTable, expected: usize) {
    let mut actual = usize::MAX;
    let wait = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            actual = count(db, table).await;
            if actual == expected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await;
    assert!(
        wait.is_ok(),
        "owned LIVE registration count did not settle: expected={expected} actual={actual}"
    );
}
async fn setup() -> fixture::TestDb {
    let db = fixture::TestDb::new().await;
    db.a.client().query("DEFINE TABLE observation_cleanup_rows SCHEMALESS; DEFINE TABLE observation_cleanup_denied SCHEMALESS; DEFINE TABLE observation_cleanup_counter SCHEMALESS; CREATE observation_cleanup_counter:fixture SET registrations = 0;").await.unwrap().check().unwrap();
    db
}
#[tokio::test]
async fn full_row_unpolled_drop_and_partial_registration_keep_parent_session() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = setup().await;
        let rows = table("observation_cleanup_rows");
        let mut stream = db.a.live::<Row>(rows.clone()).await.unwrap();
        db.b.client()
            .query("CREATE observation_cleanup_rows:fixture SET value = 42;")
            .await
            .unwrap()
            .check()
            .unwrap();
        let row = stream.next().await.unwrap().unwrap().data;
        assert_eq!(row.value, 42);
        assert_eq!(row.id.table.as_str(), rows.as_str());
        drop(stream);
        await_count(&db.a, &rows, 0).await;
        let unpolled = db.a.live::<Row>(rows.clone()).await.unwrap();
        drop(unpolled);
        await_count(&db.a, &rows, 0).await;
        let denied = table("observation_cleanup_denied");
        let result =
            db.a.registered_live::<Row>(
                &[rows.clone(), denied.clone()],
                include_str!("tests/registration.surql"),
            )
            .await;
        let error = result
            .err()
            .expect("deliberate partial registration denial");
        assert!(
            format!("{error:#}").contains("fixture registration denial"),
            "{error:#}"
        );
        await_count(&db.a, &rows, 0).await;
        await_count(&db.a, &denied, 0).await;
        db.a.client()
            .query("RETURN true;")
            .await
            .unwrap()
            .check()
            .unwrap();
    })
    .await
    .expect("owned full-row lifecycle exceeded90seconds");
}
#[tokio::test]
async fn abandoned_registration_and_unknown_receipt_close_only_owned_transport() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = setup().await;
        let rows = table("observation_cleanup_rows");
        // SLEEP is administrative SurrealQL; production/other tests use the editor.
        let admin = db.admin().await;
        let store = admin.clone();
        let owned_rows = rows.clone();
        let later_rows = table("observation_cleanup_denied");
        let task = tokio::spawn(async move {
            store
                .registered_live::<Row>(
                    &[owned_rows, later_rows],
                    include_str!("tests/delayed_registration.surql"),
                )
                .await
        });
        let mut task = task;
        tokio::select! {
            _ = await_count(&db.a, &rows, 1) => {},
            result = &mut task => match result {
                Ok(Err(error)) => panic!("registration failed before native LIVE became visible: {error:#}"),
                Ok(Ok(_stream)) => panic!("registration completed before delayed native receipt"),
                Err(error) => panic!("registration task failed: {error}"),
            },
        }
        task.abort();
        assert!(
            task.await
                .err()
                .expect("caller task cancelled")
                .is_cancelled()
        );
        await_count(&db.a, &rows, 0).await;
        let mut response =
            db.a.client()
                .query("RETURN observation_cleanup_counter:fixture.registrations;")
                .await
                .unwrap()
                .check()
                .unwrap();
        let later_registrations: Option<i64> = response.take(0).unwrap();
        assert_eq!(later_registrations, Some(0), "abandoned caller registered a later table");
        // Force the actual observer API deadline while the database has committed
        // LIVE but the delayed response has not yet returned its query UUID.
        let store = admin.clone();
        let owned_rows = rows.clone();
        let task = tokio::spawn(async move {
            store.registered_live_with_timeout::<Row>(
                &[owned_rows],
                include_str!("tests/timeout_registration.surql"),
                Duration::from_secs(5),
            ).await
        });
        let mut task = task;
        tokio::select! {
            _ = await_count(&db.a, &rows, 1) => {},
            result = &mut task => panic!("registration finished before unknown receipt was observed: {}", match result { Ok(Err(error)) => format!("{error:#}"), Ok(Ok(_)) => "unexpected success".to_owned(), Err(error) => error.to_string() }),
        }
        let registered_at = tokio::time::Instant::now();
        let error = task.await.unwrap().err().expect("registration deadline must fail");
        assert!(matches!(error, StoreError::ChangefeedConnectionTimeout), "{error:#}");
        await_count(&db.a, &rows, 0).await;
        // The server could complete its ten-second delayed statement after the
        // client deadline. Check again beyond that point to reject late leaks.
        tokio::time::sleep_until(registered_at + Duration::from_secs(11)).await;
        assert_eq!(count(&db.a, &rows).await, 0, "late LIVE query survived owned transport closure");
        db.a.client()
            .query("RETURN true;")
            .await
            .unwrap()
            .check()
            .unwrap();
    })
    .await
    .expect("abandoned LIVE registration exceeded90seconds");
}
