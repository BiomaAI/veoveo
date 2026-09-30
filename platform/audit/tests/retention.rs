//! One whole-block policy covers every record class and preserves unsealed writes.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
mod support;
use chrono::{TimeDelta, Utc};
use std::time::Duration;
use veoveo_audit::{integrity::AuditSigningKey, *};
use veoveo_platform_store::StoreError;

#[tokio::test]
async fn cutoff_retention_removes_all_classes_only_after_their_block_expires() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::with_backend(fixture::StoreBackend::RocksDb).await;
        let details = [
            AuditDetail::Read {
                method: AuditReadMethod::Status,
            },
            AuditDetail::Authentication {
                activity: AuthenticationActivity::Login,
                method: veoveo_types::AuthMethod::BearerJwt,
                reason: veoveo_types::AuthReasonCode::AuthAllow,
            },
            AuditDetail::AccountChange {
                activity: AccountActivity::Update,
            },
            AuditDetail::Artifact {
                activity: ArtifactActivity::Inspect,
                requested: None,
                subject: None,
                release_state: None,
                related: None,
                bytes: None,
                window_start: None,
            },
            AuditDetail::LiveView {
                activity: LiveViewActivity::Issue,
            },
            AuditDetail::Computer {
                activity: ComputerActivity::Create,
                stage: ComputerAuditStage::Reserved,
                task: None,
            },
        ];
        let drafts = details
            .into_iter()
            .map(|detail| {
                AuditDraft::builder(
                    AuditRequest::background(),
                    AuditTarget::Installation,
                    detail,
                    AuditOutcome::Allowed,
                    AuditReason::Accepted,
                )
                .build()
                .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            drafts
                .iter()
                .map(|d| d.detail().class())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            6
        );
        db.a.append_audit_records(&drafts).await.unwrap();
        let stored =
            db.a.audit_page(
                &AuditReadScope::new(None, true),
                &AuditQuery::new(AuditPartition::Installation),
            )
            .await
            .unwrap();
        assert_eq!(
            stored.records.len(),
            drafts.len(),
            "acknowledged audit writes must be visible to readers"
        );
        let mut lease =
            db.a.acquire_audit_seal_lease(uuid::Uuid::now_v7())
                .await
                .unwrap();
        let page = support::committed_records(&db.a, &mut lease).await;
        let records = page
            .records
            .iter()
            .map(|(_, record)| record.clone())
            .collect::<Vec<_>>();
        let members = page
            .records
            .iter()
            .map(|(stamp, record)| AuditBlockMember {
                id: record.draft.id(),
                versionstamp: *stamp,
            })
            .collect();
        let sealed_at = Utc::now();
        let block = AuditSigningKey::from_seed(&[7; 32])
            .seal(
                AuditPartition::Installation,
                None,
                members,
                &records,
                sealed_at,
            )
            .unwrap();
        db.a.commit_audit_blocks(&lease, page.next, std::slice::from_ref(&block))
            .await
            .unwrap();
        let unsealed = AuditDraft::builder(
            AuditRequest::background(),
            AuditTarget::Installation,
            AuditDetail::Read {
                method: AuditReadMethod::Status,
            },
            AuditOutcome::Allowed,
            AuditReason::Accepted,
        )
        .build()
        .unwrap();
        db.b.append_audit_records(std::slice::from_ref(&unsealed))
            .await
            .unwrap();
        assert!(
            db.b.audit_retention_candidates(sealed_at, &[])
                .await
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            db.a.retain_audit_block(&lease, &block, sealed_at, &[])
                .await,
            Err(StoreError::AuditRetentionNotAdmitted)
        ));
        let cutoff = sealed_at + TimeDelta::seconds(1);
        assert_eq!(
            db.b.audit_retention_candidates(cutoff, &[]).await.unwrap(),
            vec![block.clone()]
        );
        db.a.retain_audit_block(&lease, &block, cutoff, &[])
            .await
            .unwrap();
        let scope = AuditReadScope::new(None, true);
        let page =
            db.b.audit_page(&scope, &AuditQuery::new(AuditPartition::Installation))
                .await
                .unwrap();
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.records[0].draft, unsealed);
        let interval =
            db.b.audit_export_range(&scope, &AuditPartition::Installation)
                .await
                .unwrap();
        assert!(interval.first.is_none());
        assert_eq!(interval.checkpoint, Some(block.checkpoint()));
    })
    .await
    .expect("audit retention fixture exceeded 90 seconds");
}
