//! Committed windows survive replica changes without per-read audit records.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
use sha2::{Digest, Sha256};
use std::time::Duration;
use veoveo_audit_contract::*;
use veoveo_mcp_knowledge_extension::{Observation, Revision, content_digest, docs};
use veoveo_platform_store::PlatformStore;

fn hex(bytes: impl AsRef<[u8]>) -> String {
    veoveo_types::Sha256Digest::from_bytes(Sha256::digest(bytes).into())
        .hex()
        .to_owned()
}
fn read(not_modified: bool, failed: bool) -> IndexingRead {
    let server = "time".parse().unwrap();
    let collection = docs::collection(&server, &"time".parse().unwrap());
    let uri: veoveo_types::ResourceUri =
        veoveo_types::ResourceUri::new("time://docs/design").unwrap();
    let observation = Observation::builder(
        collection.collection().clone(),
        Revision::new("revision-1").unwrap(),
        content_digest("body"),
        chrono::Utc::now(),
    )
    .build(&collection)
    .unwrap();
    let mut observed: KnowledgeReadObservation = (&observation).into();
    observed.not_modified = not_modified;
    let (detail, outcome, reason) = if failed {
        (
            AuditDetail::Read {
                method: AuditReadMethod::ResourceRead,
            },
            AuditOutcome::Failed,
            AuditReason::UpstreamFailure,
        )
    } else {
        (
            AuditDetail::KnowledgeRead {
                member: uri.clone(),
                observation: Some(Box::new(observed)),
                status: if not_modified {
                    KnowledgeReadStatus::NotModified
                } else {
                    KnowledgeReadStatus::Read
                },
            },
            AuditOutcome::Succeeded,
            AuditReason::Accepted,
        )
    };
    let draft = AuditDraft::builder(
        AuditRequest::background(),
        AuditTarget::Resource { server, uri },
        detail,
        outcome,
        reason,
    )
    .actor(AuditActor {
        principal: "indexer".parse().unwrap(),
        kind: AuditPrincipalKind::Service,
        tenant: Some("test-tenant".parse().unwrap()),
        oauth_client: Some("indexer".parse().unwrap()),
        session_family: None,
        delegating_principal: None,
        managed_agent: None,
    })
    .build()
    .unwrap();
    IndexingRead::new(draft, collection.collection().clone()).unwrap()
}
async fn records(store: &PlatformStore) -> Vec<AuditRecord> {
    store
        .audit_page(
            &AuditReadScope::new(Some("test-tenant".parse().unwrap()), false),
            &AuditQuery::new(AuditPartition::Tenant("test-tenant".parse().unwrap())),
        )
        .await
        .unwrap()
        .records
}
async fn expire_windows(store: &PlatformStore) {
    // Move only fixture staging into the previous five-minute slot. The production
    // finalizer still uses the database clock and the real immutable append path.
    store
        .client()
        .query(
            "BEGIN TRANSACTION;
        LET $windows = SELECT * FROM audit_indexing_window;
        FOR $window IN $windows {
            DELETE ONLY $window.id;
            CREATE ONLY $window.id CONTENT object::extend($window, {
                start: $window.start - 5m, end: $window.end - 5m
            });
        }; COMMIT TRANSACTION;",
        )
        .await
        .unwrap()
        .check()
        .unwrap();
}
#[tokio::test]
async fn windows_deduplicate_retries_separate_authority_and_close_once_across_replicas() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let reads = vec![read(false, false), read(true, false), read(false, true)];
        let (a, b) = tokio::join!(
            db.a.append_audit_group(&[], &reads),
            db.b.append_audit_group(&[], &reads)
        );
        a.unwrap();
        b.unwrap();
        assert!(records(&db.b).await.is_empty(), "no per-read audit records");
        assert_eq!(
            db.b.close_audit_indexing_windows().await.unwrap(),
            0,
            "open window cannot seal"
        );
        db.b.append_audit_group(&[], &reads).await.unwrap();
        let mut altered = serde_json::to_value(reads[0].draft()).unwrap();
        altered["authority"]["profile"] = serde_json::json!("other");
        let conflict = IndexingRead::new(
            serde_json::from_value(altered.clone()).unwrap(),
            reads[0].collection().clone(),
        )
        .unwrap();
        assert!(
            db.b.append_audit_group(
                std::slice::from_ref(reads[0].draft()),
                &[read(false, false), conflict]
            )
            .await
            .is_err(),
            "changed retry rolls back whole group"
        );
        altered["id"] = serde_json::to_value(AuditRecordId::new()).unwrap();
        let other = IndexingRead::new(
            serde_json::from_value(altered).unwrap(),
            reads[0].collection().clone(),
        )
        .unwrap();
        db.a.append_audit_group(&[], &[other]).await.unwrap();
        expire_windows(&db.a).await;
        let (a, b) = tokio::join!(
            db.a.close_audit_indexing_windows(),
            db.b.close_audit_indexing_windows()
        );
        a.unwrap();
        b.unwrap();
        let sealed = records(&db.b).await;
        assert_eq!(
            sealed.len(),
            2,
            "authority changes must not merge attribution"
        );
        let summary = sealed
            .iter()
            .find(|record| record.draft.authority().profile.is_none())
            .unwrap();
        let AuditDetail::IndexingWindow {
            reads: count,
            not_modified,
            failed,
            members_digest,
            ..
        } = summary.draft.detail()
        else {
            panic!("window expected")
        };
        assert_eq!((*count, *not_modified, *failed), (3, 1, 1));
        let mut digest = hex(b"veoveo.ai/audit-indexing-members/v1");
        for _ in 0..2 {
            let leaf = hex(serde_json::to_vec(&("time://docs/design", "revision-1")).unwrap());
            digest = hex(format!(
                "veoveo.ai/audit-indexing-members/v1:{digest}{leaf}"
            ));
        }
        assert_eq!(members_digest.hex(), digest);
        db.b.append_audit_group(&[], &reads).await.unwrap();
        assert_eq!(
            db.a.close_audit_indexing_windows().await.unwrap(),
            0,
            "retry after close cannot recreate a window"
        );
        assert_eq!(records(&db.a).await.len(), 2);
        // Even after its receipt expires, a day-old read cannot be admitted again.
        let mut expired = serde_json::to_value(reads[0].draft()).unwrap();
        expired["id"] = serde_json::to_value(AuditRecordId::new()).unwrap();
        expired["occurred_at"] =
            serde_json::to_value(chrono::Utc::now() - chrono::Duration::days(2)).unwrap();
        let expired = IndexingRead::new(
            serde_json::from_value(expired).unwrap(),
            reads[0].collection().clone(),
        )
        .unwrap();
        assert!(db.a.append_audit_group(&[], &[expired]).await.is_err());
        assert_eq!(db.a.prune_audit_indexing_receipts().await.unwrap(), 0);
        // Cleanup is bounded, replica-safe and cannot remove pending accumulators.
        db.a.append_audit_group(&[], &[read(false, false)])
            .await
            .unwrap();
        db.a.client()
            .query(
                "FOR $n IN 0..1030 { CREATE type::record('audit_indexing_receipt', $n)
            SET fingerprint = 'fixture', recorded_at = time::now() - 2d; };",
            )
            .await
            .unwrap()
            .check()
            .unwrap();
        let (a, b) = tokio::join!(
            db.a.prune_audit_indexing_receipts(),
            db.b.prune_audit_indexing_receipts()
        );
        assert_eq!(a.unwrap() + b.unwrap(), 1030);
        assert_eq!(db.a.prune_audit_indexing_receipts().await.unwrap(), 0);
        let mut pending =
            db.b.client()
                .query("SELECT VALUE reads FROM audit_indexing_window;")
                .await
                .unwrap()
                .check()
                .unwrap();
        assert_eq!(pending.take::<Vec<u64>>(0).unwrap(), vec![1]);
    })
    .await
    .expect("indexing audit qualification exceeded 90 seconds");
}
