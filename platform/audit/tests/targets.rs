//! Native owner target reads, transactions and immutable registry admission.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
mod support;
use futures::StreamExt;
use std::{sync::Arc, time::Duration};
use veoveo_audit::{
    integrity::{AuditSigningKey, record_root},
    *,
};
use veoveo_computers_contract::{ComputerAuditTarget, ComputerId, register_audit_target};
use veoveo_platform_store::{
    StoreError,
    audit::{AuditLiveChange, AuditTransactionWrite},
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(from = "ShadowWire", into = "ShadowWire")]
struct ShadowTarget {
    lookup_id: uuid::Uuid,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum ShadowWire {
    #[serde(rename = "shadow_computer")]
    Shadow { lookup_id: uuid::Uuid },
}
impl From<ShadowWire> for ShadowTarget {
    fn from(wire: ShadowWire) -> Self {
        let ShadowWire::Shadow { lookup_id } = wire;
        Self { lookup_id }
    }
}
impl From<ShadowTarget> for ShadowWire {
    fn from(target: ShadowTarget) -> Self {
        Self::Shadow {
            lookup_id: target.lookup_id,
        }
    }
}
impl schemars::JsonSchema for ShadowTarget {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ShadowTarget".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type":"object","additionalProperties":false,"properties":{"kind":{"type":"string","const":"shadow_computer"},"lookup_id":{"type":"string","format":"uuid"}},"required":["kind","lookup_id"]})
    }
}
impl AuditTargetOwner for ShadowTarget {
    const KIND: &'static str = "shadow_computer";
    fn lookup_reference(&self) -> Result<Option<AuditLookupReference>, AuditTargetError> {
        Ok(Some(AuditLookupReference::new(
            "computer",
            AuditLookupKey::Uuid(self.lookup_id),
        )?))
    }
}
fn targets() -> Arc<AuditTargetRegistry> {
    let mut builder = AuditTargetRegistry::builder();
    register_audit_target(&mut builder).unwrap();
    builder.register::<ShadowTarget>().unwrap();
    Arc::new(builder.build())
}
fn draft(target: AuditTarget) -> AuditDraft {
    AuditDraft::builder(
        AuditRequest::background(),
        target,
        AuditDetail::Computer {
            activity: ComputerActivity::Create,
            stage: ComputerAuditStage::Reserved,
            task: None,
        },
        AuditOutcome::Succeeded,
        AuditReason::Accepted,
    )
    .build()
    .unwrap()
}

#[tokio::test]
async fn targets_preserve_exact_filter_transactions_live_sealing_and_export() {
    let qualification = async {
        let registry = targets();
        let db = fixture::TestDb::with_audit_targets(registry.clone()).await;
        let mut table_info =
            db.a.client()
                .query(include_str!("queries/targets/targets_preserve_exact_filter_transactions_live_sealing_and_export.surql"))
                .await
                .unwrap()
                .check()
                .unwrap();
        assert!(
            !table_info.take::<Option<bool>>(1).unwrap().unwrap(),
            "codec registration must not install Computer tables"
        );
        let selected = ComputerId::new();
        let other = ComputerId::new();
        let target = registry
            .target(ComputerAuditTarget { computer: selected })
            .unwrap();
        let records = vec![
            draft(target.clone()),
            draft(
                registry
                    .target(ShadowTarget {
                        lookup_id: selected.as_uuid(),
                    })
                    .unwrap(),
            ),
            draft(
                registry
                    .target(ComputerAuditTarget { computer: other })
                    .unwrap(),
            ),
            draft(target.clone()),
        ];
        let scope = AuditReadScope::new(None, true);
        let partition = AuditPartition::Installation;
        let mut live = db.b.audit_live(&scope, &partition).await.unwrap();
        db.a.append_audit_records(&records).await.unwrap();
        let notice = tokio::time::timeout(Duration::from_secs(10), live.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let AuditLiveChange::Record(notice) = notice else {
            panic!("expected committed owner target")
        };
        assert!(
            records
                .iter()
                .any(|record| record.id() == notice.draft.id())
        );
        drop(live);
        let mut query = AuditQuery::new(partition.clone());
        query.target = Some(target.clone());
        query.limit = 1;
        let first = db.b.audit_page(&scope, &query).await.unwrap();
        assert_eq!(first.records.len(), 1);
        assert_eq!(first.records[0].draft.target(), &target);
        assert!(first.next.is_some());
        let reconnected = db.connect_at(db.a.config().endpoint().as_str()).await;
        assert!(std::ptr::eq(
            db.a.audit_targets(),
            reconnected.audit_targets()
        ));
        let recovered = reconnected.audit_page(&scope, &query).await.unwrap();
        assert_eq!(recovered.records[0].draft.target(), &target);
        query.cursor = first.next.clone();
        let second = db.b.audit_page(&scope, &query).await.unwrap();
        assert_eq!(second.records.len(), 1);
        assert_eq!(second.records[0].draft.target(), &target);
        assert_ne!(second.records[0].draft.id(), first.records[0].draft.id());
        assert!(second.next.is_none());
        let alien = targets()
            .target(ComputerAuditTarget { computer: selected })
            .unwrap();
        query.cursor = None;
        query.target = Some(alien.clone());
        assert!(matches!(
            db.a.audit_page(&scope, &query).await,
            Err(StoreError::AuditTarget(AuditTargetError::Registry))
        ));
        assert!(matches!(
            db.a.append_audit_records(&[draft(alien.clone())]).await,
            Err(StoreError::AuditTarget(AuditTargetError::Registry))
        ));
        assert!(matches!(
            AuditTransactionWrite::new(db.a.audit_targets(), draft(alien)),
            Err(StoreError::AuditTarget(AuditTargetError::Registry))
        ));
        db.admin()
            .await
            .client()
            .query(include_str!("queries/targets/define_rollback.surql"))
            .await
            .unwrap()
            .check()
            .unwrap();
        let conflict = AuditDraft::builder(
            records[0].request().clone(),
            target.clone(),
            records[0].detail().clone(),
            AuditOutcome::Failed,
            AuditReason::Unavailable,
        )
        .identity(records[0].id())
        .occurred_at(records[0].occurred_at())
        .build()
        .unwrap();
        let result=db.a.client().query(include_str!("queries/targets/targets_preserve_exact_filter_transactions_live_sealing_and_export_2.surql")).bind(AuditTransactionWrite::batch(db.a.audit_targets(),vec![draft(target.clone()),conflict]).unwrap().into_binding()).await.unwrap().check();
        assert!(result.is_err());
        let mut response =
            db.b.client()
                .query(include_str!("queries/targets/targets_preserve_exact_filter_transactions_live_sealing_and_export_3.surql"))
                .await
                .unwrap()
                .check()
                .unwrap();
        assert_eq!(response.take::<Option<u64>>(0).unwrap().unwrap(), 0);
        let mut all = AuditQuery::new(partition.clone());
        let stored = db.b.audit_page(&scope, &all).await.unwrap();
        assert_eq!(stored.records.len(), 4);
        let mut lease =
            db.a.acquire_audit_seal_lease(uuid::Uuid::now_v7())
                .await
                .unwrap();
        let page = support::committed_records(&db.a, &mut lease).await;
        assert_eq!(page.records.len(), 4);
        let admitted = page
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
        let block = AuditSigningKey::from_seed(&[71; 32])
            .seal(
                partition.clone(),
                None,
                members,
                &admitted,
                chrono::Utc::now(),
            )
            .unwrap();
        db.a.commit_audit_blocks(&lease, page.next, std::slice::from_ref(&block))
            .await
            .unwrap();
        let exported = db.b.audit_block_records(&scope, &block).await.unwrap();
        assert_eq!(record_root(&exported).unwrap(), block.head.root);
        all.target = Some(target);
        let filtered =
            db.b.audit_filtered_block_records(&scope, &block, &all)
                .await
                .unwrap();
        assert_eq!(filtered.len(), 2);
        for record in exported {
            let line = AuditExportLine::Record {
                record: Box::new(record),
            };
            let bytes = serde_json::to_string(&line).unwrap();
            assert!(matches!(
                db.b.audit_targets()
                    .decoder()
                    .from_str::<AuditExportLine>(&bytes)
                    .unwrap(),
                AuditExportLine::Record { .. }
            ));
        }
        // Recreate only the lookahead row with an unbound target in this isolated fixture.
        let extra = stored.records[1].draft.id();
        db.a.client().query(include_str!("queries/targets/targets_preserve_exact_filter_transactions_live_sealing_and_export_4.surql")).bind(("id",veoveo_platform_store::audit::record_id(&partition,extra))).await.unwrap().check().unwrap();
        let mut lookahead = AuditQuery::new(partition);
        lookahead.limit = 1;
        assert!(matches!(
            db.b.audit_page(&scope, &lookahead).await,
            Err(StoreError::AuditTarget(AuditTargetError::Unbound(_)))
        ));
    };
    tokio::time::timeout(Duration::from_secs(90), qualification)
        .await
        .expect("native Audit target qualification exceeded 90 seconds");
}
