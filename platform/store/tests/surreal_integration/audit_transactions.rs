//! Audit failures abort the caller's domain write, while identical retries deduplicate.
use super::fixture::{StoreBackend, TestDb};
use std::time::Duration;
use veoveo_audit_contract::*;
use veoveo_platform_store::{PlatformStore, audit::AuditTransactionWrite};

fn draft() -> AuditDraft {
    AuditDraft::builder(
        AuditRequest::background(),
        AuditTarget::WorkContext {
            tenant: "test-tenant".parse().unwrap(),
            context: "operations".parse().unwrap(),
        },
        AuditDetail::AccountChange {
            activity: AccountActivity::Create,
        },
        AuditOutcome::Succeeded,
        AuditReason::Accepted,
    )
    .build()
    .unwrap()
}
async fn counts(store: &PlatformStore) -> (u64, u64) {
    let mut rows = store
        .client()
        .query(include_str!(
            "../queries/surreal_integration/audit_transactions/counts.surql"
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    (
        rows.take::<Option<u64>>(0).unwrap().unwrap(),
        rows.take::<Option<u64>>(1).unwrap().unwrap(),
    )
}

#[tokio::test]
async fn domain_and_audit_commit_or_rollback_together_and_retries_keep_one_record() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::with_backend(StoreBackend::RocksDb).await;
        db.a.client()
            .query(include_str!("../queries/surreal_integration/audit_transactions/domain_and_audit_commit_or_rollback_together_and_retries_keep_one_record.surql"))
            .await
            .unwrap()
            .check()
            .unwrap();
        let record = draft();
        let result =
            db.a.client()
                .query(
                    include_str!("../queries/surreal_integration/audit_transactions/domain_and_audit_commit_or_rollback_together_and_retries_keep_one_record_2.surql"),
                )
                .bind(
                    AuditTransactionWrite::new(db.a.audit_targets(), record.clone())
                        .unwrap()
                        .into_binding(),
                )
                .await
                .unwrap()
                .check();
        assert!(result.is_err());
        assert_eq!(counts(&db.b).await, (0, 0));

        db.a.client()
            .query(
                include_str!("../queries/surreal_integration/audit_transactions/domain_and_audit_commit_or_rollback_together_and_retries_keep_one_record_3.surql"),
            )
            .bind(
                AuditTransactionWrite::new(db.a.audit_targets(), record.clone())
                    .unwrap()
                    .into_binding(),
            )
            .await
            .unwrap()
            .check()
            .unwrap();
        db.b.append_audit_records(std::slice::from_ref(&record))
            .await
            .unwrap();
        assert_eq!(
            counts(&db.a).await,
            (1, 1),
            "retries preserve one domain row and one audit record"
        );

        let conflict = AuditDraft::builder(
            record.request().clone(),
            record.target().clone(),
            record.detail().clone(),
            AuditOutcome::Failed,
            AuditReason::Unavailable,
        )
        .identity(record.id())
        .occurred_at(record.occurred_at())
        .build()
        .unwrap();
        let result =
            db.a.client()
                .query(
                    include_str!("../queries/surreal_integration/audit_transactions/domain_and_audit_commit_or_rollback_together_and_retries_keep_one_record_4.surql"),
                )
                .bind(
                    AuditTransactionWrite::batch(db.a.audit_targets(), vec![draft(), conflict])
                        .unwrap()
                        .into_binding(),
                )
                .await
                .unwrap()
                .check();
        assert!(
            result.is_err(),
            "a different draft cannot reuse an immutable audit identity"
        );
        assert_eq!(
            counts(&db.b).await,
            (1, 1),
            "both the domain write and earlier batch members roll back"
        );
    })
    .await
    .expect("transactional audit qualification exceeded 90 seconds");
}
