//! Acknowledged indexing reads survive writer replacement and enter signed blocks.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
use std::{num::NonZeroU32, sync::Arc, time::Duration};
use veoveo_audit::{integrity::AuditSigningKey, *};

#[tokio::test]
async fn indexing_acknowledgements_survive_restart_and_windows_are_sealed() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let draft = AuditDraft::builder(
            AuditRequest::background(),
            AuditTarget::Resource {
                server: "time".parse().unwrap(),
                uri: veoveo_types::ResourceUri::new("time://docs").unwrap(),
            },
            AuditDetail::Read {
                method: AuditReadMethod::ResourceRead,
            },
            AuditOutcome::Succeeded,
            AuditReason::Accepted,
        )
        .actor(AuditActor {
            principal: "indexer".parse().unwrap(),
            kind: AuditPrincipalKind::Service,
            tenant: Some("tenant".parse().unwrap()),
            oauth_client: Some("indexer".parse().unwrap()),
            session_family: None,
            delegating_principal: None,
            managed_agent: None,
        })
        .build()
        .unwrap();
        let read = IndexingRead::new(draft, "time.docs".parse().unwrap()).unwrap();
        let writer = AuditWriter::start(db.a.clone());
        writer.record_indexing(read.clone()).await.unwrap();
        let mut result =
            db.b.client()
                .query(include_str!("queries/indexing/indexing_acknowledgements_survive_restart_and_windows_are_sealed.surql"))
                .await
                .unwrap()
                .check()
                .unwrap();
        assert_eq!(
            result.take::<Vec<u64>>(0).unwrap(),
            vec![1],
            "ack must follow durable accumulation"
        );
        writer.shutdown(Duration::from_secs(10)).await.unwrap();
        assert!(matches!(
            writer.record_indexing(read.clone()).await,
            Err(AuditWriteError::Closed)
        ));
        db.b.client()
            .query(
                include_str!("queries/indexing/indexing_acknowledgements_survive_restart_and_windows_are_sealed_2.surql"),
            )
            .await
            .unwrap()
            .check()
            .unwrap();
        let successor = AuditWriter::start(db.b.clone());
        let service = AuditService::start(
            db.a.clone(),
            Arc::new(AuditSigningKey::from_seed(&[37; 32])),
            NonZeroU32::new(1).unwrap(),
            Default::default(),
        )
        .unwrap();
        let partition = AuditPartition::Tenant("tenant".parse().unwrap());
        let scope = AuditReadScope::new(Some("tenant".parse().unwrap()), false);
        let summary = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let page =
                    db.a.audit_page(&scope, &AuditQuery::new(partition.clone()))
                        .await
                        .unwrap();
                if let Some(record) = page.records.into_iter().next() {
                    break record;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
        assert!(matches!(
            summary.draft.detail(),
            AuditDetail::IndexingWindow { reads: 1, .. }
        ));
        successor.record_indexing(read).await.unwrap();
        tokio::time::timeout(
            Duration::from_secs(10),
            db.a.audit_wait_sealed(&scope, &partition, summary.draft.id()),
        )
        .await
        .unwrap()
        .unwrap();
        successor.shutdown(Duration::from_secs(10)).await.unwrap();
        service.shutdown(Duration::from_secs(15)).await.unwrap();
        assert_eq!(
            db.a.audit_page(&scope, &AuditQuery::new(partition))
                .await
                .unwrap()
                .records
                .len(),
            1
        );
    })
    .await
    .expect("indexing writer restart fixture exceeded 90 seconds");
}
