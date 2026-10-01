//! Independent replicas preserve immutable draft identities without an audit outbox.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
use std::time::Duration;
use veoveo_audit_contract::*;
use veoveo_platform_store::PlatformStore;

fn draft(method: AuditReadMethod) -> AuditDraft {
    AuditDraft::builder(
        AuditRequest::background(),
        AuditTarget::Installation,
        AuditDetail::Read { method },
        AuditOutcome::Allowed,
        AuditReason::Accepted,
    )
    .build()
    .unwrap()
}
async fn write(store: &PlatformStore, records: &[AuditDraft]) {
    for batch in records.chunks(64) {
        store.append_audit_records(batch).await.unwrap();
    }
}
#[tokio::test]
async fn batches_commit_across_replicas_and_conflicting_identity_rolls_back_the_batch() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let first = (0..65)
            .map(|_| draft(AuditReadMethod::ResourceRead))
            .collect::<Vec<_>>();
        let second = (0..65)
            .map(|_| draft(AuditReadMethod::Status))
            .collect::<Vec<_>>();
        tokio::join!(write(&db.a, &first), write(&db.b, &second));
        let scope = AuditReadScope::new(None, true);
        let mut query = AuditQuery::new(AuditPartition::Installation);
        query.limit = 1000;
        let stored = db.a.audit_page(&scope, &query).await.unwrap();
        assert!(stored.next.is_none());
        assert_eq!(stored.records.len(), 130);
        for expected in first.iter().chain(&second) {
            assert_eq!(
                &stored
                    .records
                    .iter()
                    .find(|r| r.draft.id() == expected.id())
                    .unwrap()
                    .draft,
                expected
            );
        }
        write(&db.b, &first).await;
        assert_eq!(
            db.a.audit_page(&scope, &query).await.unwrap().records.len(),
            130,
            "identical retry is idempotent"
        );
        let pending = draft(AuditReadMethod::ResourceRead);
        let conflict = AuditDraft::builder(
            first[0].request().clone(),
            AuditTarget::Installation,
            AuditDetail::Read {
                method: AuditReadMethod::Status,
            },
            AuditOutcome::Allowed,
            AuditReason::Accepted,
        )
        .identity(first[0].id())
        .occurred_at(first[0].occurred_at())
        .build()
        .unwrap();
        assert!(
            db.a.append_audit_records(&[pending.clone(), conflict])
                .await
                .is_err()
        );
        let after = db.b.audit_page(&scope, &query).await.unwrap();
        assert_eq!(after.records.len(), 130);
        assert!(
            after
                .records
                .iter()
                .all(|record| record.draft.id() != pending.id())
        );
    })
    .await
    .expect("audit batch fixture exceeded 90 seconds");
}
