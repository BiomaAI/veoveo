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
                    include_str!("../queries/surreal_integration/query_semantics/conditional_writes_test_the_stored_value_before_applying_changes.surql"),
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
            .query(include_str!("../queries/surreal_integration/query_semantics/locked_exact_reads_conflict_with_concurrent_updates_and_insertions.surql"))
            .await
            .unwrap()
            .check()
            .unwrap();
        for present in [false, true] {
            db.a.client()
                .query(include_str!("../queries/surreal_integration/query_semantics/locked_exact_reads_conflict_with_concurrent_updates_and_insertions_2.surql"))
                .await
                .unwrap()
                .check()
                .unwrap();
            if present {
                db.a.client()
                    .query(include_str!("../queries/surreal_integration/query_semantics/locked_exact_reads_conflict_with_concurrent_updates_and_insertions_3.surql"))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            }
            let transaction = db.a.client().clone().begin().await.unwrap();
            transaction
                .query(include_str!("../queries/surreal_integration/query_semantics/locked_exact_reads_conflict_with_concurrent_updates_and_insertions_4.surql"))
                .await
                .unwrap()
                .check()
                .unwrap();
            db.b.client()
                .query(include_str!("../queries/surreal_integration/query_semantics/locked_exact_reads_conflict_with_concurrent_updates_and_insertions_5.surql"))
                .await
                .unwrap()
                .check()
                .unwrap();
            transaction
                .query(include_str!("../queries/surreal_integration/query_semantics/locked_exact_reads_conflict_with_concurrent_updates_and_insertions_6.surql"))
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
                    .query(include_str!("../queries/surreal_integration/query_semantics/locked_exact_reads_conflict_with_concurrent_updates_and_insertions_7.surql"))
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
