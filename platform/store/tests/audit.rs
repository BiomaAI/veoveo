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

#[tokio::test]
async fn whole_lookup_receipts_are_read_only_and_checked_before_replay_or_paging() {
    use surrealdb::types::{SurrealValue, Value};
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let original = draft(AuditReadMethod::ResourceRead);
        db.a.append_audit_records(std::slice::from_ref(&original))
            .await
            .unwrap();
        let scope = AuditReadScope::new(None, true);
        let mut query = AuditQuery::new(AuditPartition::Installation);
        query.target = Some(AuditTarget::Installation);
        query.limit = 1;
        let record =
            db.a.audit_page(&scope, &query)
                .await
                .unwrap()
                .records
                .remove(0);
        assert_eq!(record.draft, original);
        let canonical = serde_json::to_vec(&record).unwrap();
        let id = veoveo_platform_store::audit::record_id(original.partition(), original.id());
        let mut read =
            db.a.client()
                .query(include_str!("queries/audit/lookup_row.surql"))
                .bind(("id", id.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
        let retained: Option<Value> = read.take(0).unwrap();
        let Value::Object(retained) = retained.unwrap() else {
            panic!("Audit row object")
        };
        for (field, value) in [
            ("profile_lookup", "changed-profile".into_value()),
            (
                "target_lookup",
                surrealdb::types::SerdeWrapper(
                    serde_json::to_value(AuditTarget::Server {
                        server: "time".parse().unwrap(),
                    })
                    .unwrap(),
                )
                .into_value(),
            ),
            (
                "detail_lookup",
                surrealdb::types::SerdeWrapper(
                    serde_json::to_value(AuditDetail::Read {
                        method: AuditReadMethod::Status,
                    })
                    .unwrap(),
                )
                .into_value(),
            ),
        ] {
            let mut patch = surrealdb::types::Object::new();
            patch.insert(field, value.clone());
            assert!(
                db.a.client()
                    .query(include_str!("queries/audit/lookup_rejected_update.surql"))
                    .bind(("id", id.clone()))
                    .bind(("patch", patch.into_value()))
                    .await
                    .unwrap()
                    .check()
                    .is_err()
            );
            assert_eq!(
                serde_json::to_vec(&db.b.audit_page(&scope, &query).await.unwrap().records[0])
                    .unwrap(),
                canonical,
                "rejected lookup mutation preserves frozen record bytes"
            );
            // A privileged fixture can insert inconsistent initial metadata; every reader must reject it.
            let mut corrupt = retained.clone();
            corrupt.insert(field, value);
            db.a.client()
                .query(include_str!("queries/audit/lookup_replace.surql"))
                .bind(("id", id.clone()))
                .bind(("row", corrupt.into_value()))
                .await
                .unwrap()
                .check()
                .unwrap();
            let all = AuditQuery::new(AuditPartition::Installation);
            assert!(
                db.b.audit_page(&scope, &all).await.is_err(),
                "{field} agreement must be checked after SQL selection"
            );
            let pending = draft(AuditReadMethod::Status);
            assert!(
                db.a.append_audit_records(&[pending.clone(), original.clone()])
                    .await
                    .is_err(),
                "whole lookup replay conflict rolls back the batch"
            );
            db.a.client()
                .query(include_str!("queries/audit/lookup_replace.surql"))
                .bind(("id", id.clone()))
                .bind(("row", retained.clone().into_value()))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert_eq!(
                db.b.audit_page(&scope, &all).await.unwrap().records.len(),
                1
            );
        }

        let actor: veoveo_types::PrincipalId = "viewer".parse().unwrap();
        let profile: veoveo_types::GatewayProfileId = "audit-reader".parse().unwrap();
        let view = AuditDraft::builder(
            AuditRequest::background(),
            AuditTarget::AuditLog {
                partition: AuditPartition::Installation,
            },
            AuditDetail::Read {
                method: AuditReadMethod::AuditView,
            },
            AuditOutcome::Allowed,
            AuditReason::Accepted,
        )
        .actor(AuditActor {
            principal: actor.clone(),
            kind: AuditPrincipalKind::Service,
            tenant: None,
            oauth_client: None,
            session_family: None,
            delegating_principal: None,
            managed_agent: None,
        })
        .authority(AuditAuthority {
            profile: Some(profile.clone()),
            ..Default::default()
        })
        .build()
        .unwrap();
        db.a.append_audit_records(std::slice::from_ref(&view))
            .await
            .unwrap();
        assert!(
            db.b.audit_view_admitted(
                &scope,
                &AuditPartition::Installation,
                &AuditPartition::Installation,
                &actor,
                &profile,
                view.id()
            )
            .await
            .unwrap()
        );
        assert!(
            !db.b
                .audit_view_admitted(
                    &scope,
                    &AuditPartition::Installation,
                    &AuditPartition::Installation,
                    &actor,
                    &"other-profile".parse().unwrap(),
                    view.id()
                )
                .await
                .unwrap()
        );
        let id = veoveo_platform_store::audit::record_id(view.partition(), view.id());
        let mut response =
            db.a.client()
                .query(include_str!("queries/audit/lookup_row.surql"))
                .bind(("id", id.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
        let Value::Object(mut row) = response.take::<Option<Value>>(0).unwrap().unwrap() else {
            panic!("view row")
        };
        let changed = AuditDraft::builder(
            view.request().clone(),
            view.target().clone(),
            AuditDetail::Read {
                method: AuditReadMethod::Status,
            },
            AuditOutcome::Allowed,
            AuditReason::Accepted,
        )
        .actor(view.actor().unwrap().clone())
        .authority(view.authority().clone())
        .identity(view.id())
        .occurred_at(view.occurred_at())
        .build()
        .unwrap();
        row.insert(
            "draft",
            surrealdb::types::SerdeWrapper(serde_json::to_value(changed).unwrap()).into_value(),
        );
        db.a.client()
            .query(include_str!("queries/audit/lookup_replace.surql"))
            .bind(("id", id))
            .bind(("row", row.into_value()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            db.b.audit_view_admitted(
                &scope,
                &AuditPartition::Installation,
                &AuditPartition::Installation,
                &actor,
                &profile,
                view.id()
            )
            .await
            .is_err(),
            "matching lookup predicates cannot admit a disagreeing frozen draft"
        );
    })
    .await
    .expect("Audit lookup checks exceeded 90 seconds");
}
