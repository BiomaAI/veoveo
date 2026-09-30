//! The installed database's conditional-write and locked-read guarantees.
use super::fixture::{StoreBackend, TestDb};
use std::time::Duration;

#[tokio::test]
async fn conditional_writes_test_the_stored_value_before_applying_changes() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::with_backend(StoreBackend::RocksDb).await;
        let mut response =
            db.a.client()
                .query(
                    "CREATE upgrade_guard:one SET revision = 1;
             UPDATE upgrade_guard:one SET revision = 2 WHERE revision = 1;
             UPSERT upgrade_guard:one SET revision = 3 WHERE revision = 2;
             UPDATE upgrade_guard:one SET revision = 4 WHERE revision = 4;
             UPSERT upgrade_guard:one SET revision = 5 WHERE revision = 5;
             SELECT VALUE revision FROM ONLY upgrade_guard:one;",
                )
                .await
                .unwrap()
                .check()
                .unwrap();
        let revision: Option<i64> = response.take(5).unwrap();
        assert_eq!(revision, Some(3), "WHERE must see the pre-write revision");
    })
    .await
    .expect("conditional-write qualification exceeded 90 seconds");
}

#[tokio::test]
async fn locked_exact_reads_conflict_with_concurrent_updates_and_insertions() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::with_backend(StoreBackend::RocksDb).await;
        db.a.client()
            .query("DEFINE TABLE upgrade_pointer SCHEMALESS; DEFINE TABLE upgrade_decision SCHEMALESS;")
            .await
            .unwrap()
            .check()
            .unwrap();
        for present in [false, true] {
            db.a.client()
                .query("DELETE upgrade_pointer:one; DELETE upgrade_decision:one;")
                .await
                .unwrap()
                .check()
                .unwrap();
            if present {
                db.a.client()
                    .query("CREATE upgrade_pointer:one SET revision = 1;")
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            }
            let transaction = db.a.client().clone().begin().await.unwrap();
            transaction
                .query("SELECT * FROM ONLY upgrade_pointer:one FOR UPDATE;")
                .await
                .unwrap()
                .check()
                .unwrap();
            db.b.client()
                .query("UPSERT upgrade_pointer:one SET revision = 2;")
                .await
                .unwrap()
                .check()
                .unwrap();
            transaction
                .query("CREATE upgrade_decision:one SET accepted = true;")
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(
                transaction.commit().await.is_err(),
                "a locked read must detect a changed pointer, present={present}"
            );
            let count: Option<i64> =
                db.b.client()
                    .query("RETURN array::len(SELECT * FROM upgrade_decision);")
                    .await
                    .unwrap()
                    .check()
                    .unwrap()
                    .take(0)
                    .unwrap();
            assert_eq!(count, Some(0), "a conflicted decision must roll back");
        }
    })
    .await
    .expect("locked-read qualification exceeded 90 seconds");
}
