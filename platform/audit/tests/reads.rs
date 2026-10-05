//! Database filtering, view admission and LIVE recovery for the Console and CLI.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

use chrono::{DateTime, TimeDelta, Utc};
use futures::StreamExt;
use std::{collections::BTreeSet, num::NonZeroU32, sync::Arc, time::Duration};
use veoveo_audit::{integrity::AuditSigningKey, *};
use veoveo_platform_store::{StoreError, audit::AuditLiveChange};

fn actor(tenant: &str, name: &str) -> AuditActor {
    AuditActor {
        principal: name.parse().unwrap(),
        kind: AuditPrincipalKind::User,
        tenant: Some(tenant.parse().unwrap()),
        oauth_client: None,
        session_family: None,
        delegating_principal: None,
        managed_agent: None,
    }
}

fn draft(
    actor: &AuditActor,
    request: &AuditRequest,
    time: DateTime<Utc>,
    outcome: AuditOutcome,
) -> AuditDraft {
    AuditDraft::builder(
        request.clone(),
        AuditTarget::Installation,
        AuditDetail::Read {
            method: AuditReadMethod::ResourceRead,
        },
        outcome,
        if outcome == AuditOutcome::Allowed {
            AuditReason::Accepted
        } else {
            AuditReason::PolicyDenied
        },
    )
    .actor(actor.clone())
    .occurred_at(time)
    .build()
    .unwrap()
}

#[tokio::test]
async fn sql_filters_before_decoding_and_limits_and_binds_view_receipts_to_current_identity() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let alice = actor("alpha", "alice");
        let partition = AuditPartition::Tenant("alpha".parse().unwrap());
        let scope = AuditReadScope::new(Some("alpha".parse().unwrap()), true);
        let request = AuditRequest::background();
        let now = Utc::now();
        let first = draft(&alice, &request, now, AuditOutcome::Allowed);
        let denied = draft(&alice, &request, now, AuditOutcome::Denied);
        let last = draft(&alice, &request, now, AuditOutcome::Allowed);
        db.a.append_audit_records(&[first.clone(), denied, last.clone()])
            .await
            .unwrap();
        // A known nonmatching class deliberately cannot decode as an AuditDraft.
        // Successful filtered reads therefore prove admission happened inside SQL.
        db.a.client()
            .query(
                include_str!("queries/reads/sql_filters_before_decoding_and_limits_and_binds_view_receipts_to_current_identity.surql"),
            )
            .bind(("trace", request.trace_id.to_string()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let mut query = AuditQuery::new(partition.clone());
        query.limit = 1;
        query.class = Some(AuditClass::ApiActivity);
        query.outcome = Some(AuditOutcome::Allowed);
        query.actor = Some(alice.principal.clone());
        query.target = Some(AuditTarget::Installation);
        query.trace = Some(request.trace_id.clone());
        query.from = Some(now - TimeDelta::seconds(1));
        query.until = Some(now + TimeDelta::seconds(1));
        for (order, expected) in [
            (AuditOrder::OldestFirst, [first.id(), last.id()]),
            (AuditOrder::NewestFirst, [last.id(), first.id()]),
        ] {
            query.order = order;
            query.cursor = None;
            let page = db.b.audit_page(&scope, &query).await.unwrap();
            assert_eq!(page.records.len(), 1);
            assert_eq!(page.records[0].draft.id(), expected[0]);
            query.cursor = Some(page.next.expect("matching second row supplies a cursor"));
            let page = db.b.audit_page(&scope, &query).await.unwrap();
            assert_eq!(page.records.len(), 1);
            assert_eq!(page.records[0].draft.id(), expected[1]);
            assert!(page.next.is_none());
        }
        let foreign = AuditReadScope::new(Some("bravo".parse().unwrap()), false);
        assert!(matches!(
            db.b.audit_page(&foreign, &query).await,
            Err(StoreError::AuditAccessDenied)
        ));

        let profile: veoveo_types::GatewayProfileId = "operator".parse().unwrap();
        for (age, admitted) in [(0, true), (30, false)] {
            let receipt = AuditDraft::builder(
                AuditRequest::background(),
                AuditTarget::AuditLog {
                    partition: partition.clone(),
                },
                AuditDetail::Read {
                    method: AuditReadMethod::AuditView,
                },
                AuditOutcome::Allowed,
                AuditReason::Accepted,
            )
            .actor(alice.clone())
            .authority(AuditAuthority {
                profile: Some(profile.clone()),
                ..Default::default()
            })
            .occurred_at(now - TimeDelta::minutes(age))
            .build()
            .unwrap();
            db.a.append_audit_records(std::slice::from_ref(&receipt))
                .await
                .unwrap();
            assert_eq!(
                db.b.audit_view_admitted(
                    &scope,
                    &partition,
                    &partition,
                    &alice.principal,
                    &profile,
                    receipt.id()
                )
                .await
                .unwrap(),
                admitted
            );
            assert!(
                !db.b
                    .audit_view_admitted(
                        &scope,
                        &partition,
                        &partition,
                        &"bob".parse().unwrap(),
                        &profile,
                        receipt.id()
                    )
                    .await
                    .unwrap()
            );
            assert!(
                !db.b
                    .audit_view_admitted(
                        &scope,
                        &partition,
                        &partition,
                        &alice.principal,
                        &"other".parse().unwrap(),
                        receipt.id()
                    )
                    .await
                    .unwrap()
            );
            assert!(
                !db.b
                    .audit_view_admitted(
                        &scope,
                        &AuditPartition::Installation,
                        &partition,
                        &alice.principal,
                        &profile,
                        receipt.id()
                    )
                    .await
                    .unwrap()
            );
        }
    })
    .await
    .expect("audit query and view admission exceeded 90 seconds");
}

#[tokio::test]
async fn scoped_live_daily_pages_and_sealed_recovery_preserve_partition_and_date_bounds() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::with_backend(fixture::StoreBackend::RocksDb).await;
        let alice = actor("alpha", "alice");
        let partition = AuditPartition::Tenant("alpha".parse().unwrap());
        let scope = AuditReadScope::new(Some("alpha".parse().unwrap()), false);
        let today = Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let now = Utc::now();
        let request = AuditRequest::background();
        let expected = [
            draft(&alice, &request, now, AuditOutcome::Allowed),
            draft(&alice, &request, now, AuditOutcome::Denied),
            draft(
                &alice,
                &request,
                today - TimeDelta::hours(1),
                AuditOutcome::Allowed,
            ),
            draft(&alice, &request, now, AuditOutcome::Allowed),
        ];
        let mut live = db.b.audit_live(&scope, &partition).await.unwrap();
        let mut sealed = db.b.audit_blocks_live(&scope, &partition).await.unwrap();
        let service = AuditService::start(
            db.a.clone(),
            Arc::new(AuditSigningKey::from_seed(&[41; 32])),
            NonZeroU32::new(7).unwrap(),
            Default::default(),
        )
        .unwrap();
        let foreign = draft(&actor("bravo", "bob"), &request, now, AuditOutcome::Allowed);
        db.a.append_audit_records(&[foreign]).await.unwrap();
        db.a.append_audit_records(&expected).await.unwrap();
        let mut received = BTreeSet::new();
        for _ in &expected {
            let change = tokio::time::timeout(Duration::from_secs(10), live.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let AuditLiveChange::Record(record) = change else {
                panic!("create produced retention notification");
            };
            assert_eq!(record.draft.partition(), &partition);
            received.insert(record.draft.id());
        }
        assert_eq!(received, expected.iter().map(AuditDraft::id).collect());
        tokio::time::timeout(Duration::from_secs(20), sealed.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let blocks =
            db.b.audit_blocks(&scope, &partition, None, 10)
                .await
                .unwrap();
        assert_eq!(blocks.len(), 1);
        let range = db.b.audit_export_range(&scope, &partition).await.unwrap();
        assert_eq!(range.first, Some(blocks[0].head.sequence));
        assert_eq!(range.checkpoint, Some(blocks[0].checkpoint()));
        let mut filter = AuditQuery::new(partition.clone());
        filter.outcome = Some(AuditOutcome::Denied);
        let records =
            db.b.audit_filtered_block_records(&scope, &blocks[0], &filter)
                .await
                .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].draft.id(), expected[1].id());

        let mut daily = AuditDailyQuery {
            partition: partition.clone(),
            from: today - TimeDelta::days(1),
            until: today + TimeDelta::days(1),
            cursor: None,
            limit: 1,
        };
        let mut counts = Vec::new();
        loop {
            let page = db.b.audit_daily(&scope, &daily).await.unwrap();
            assert!(page.counts.iter().all(|count| count.partition == partition));
            counts.extend(
                page.counts
                    .into_iter()
                    .map(|count| (count.day, count.outcome, count.count)),
            );
            let Some(next) = page.next else {
                break;
            };
            daily.cursor = Some(next);
        }
        assert_eq!(
            counts,
            vec![
                (today, AuditOutcome::Allowed, 2),
                (today, AuditOutcome::Denied, 1),
                (today - TimeDelta::days(1), AuditOutcome::Allowed, 1)
            ]
        );
        daily.from = today;
        daily.cursor = None;
        daily.limit = 100;
        assert_eq!(
            db.b.audit_daily(&scope, &daily)
                .await
                .unwrap()
                .counts
                .iter()
                .map(|count| count.count)
                .sum::<u64>(),
            3
        );

        drop(live);
        drop(sealed);
        let missed = draft(&alice, &request, now, AuditOutcome::Allowed);
        db.a.append_audit_records(std::slice::from_ref(&missed))
            .await
            .unwrap();
        tokio::time::timeout(
            Duration::from_secs(15),
            db.b.audit_wait_sealed(&scope, &partition, missed.id()),
        )
        .await
        .unwrap()
        .unwrap();
        // A resumed subscriber opens LIVE first, then replays its partition's signed blocks.
        let mut resumed = db.b.audit_live(&scope, &partition).await.unwrap();
        let recovered =
            db.b.audit_blocks(&scope, &partition, Some(blocks[0].head.sequence), 10)
                .await
                .unwrap();
        assert_eq!(recovered.len(), 1);
        let recovered_records =
            db.b.audit_block_records(&scope, &recovered[0])
                .await
                .unwrap();
        assert_eq!(recovered_records.len(), 1);
        assert_eq!(recovered_records[0].draft.id(), missed.id());
        service.shutdown(Duration::from_secs(15)).await.unwrap();
        db.a.client()
            .query(include_str!("queries/reads/scoped_live_daily_pages_and_sealed_recovery_preserve_partition_and_date_bounds.surql"))
            .bind((
                "id",
                veoveo_platform_store::audit::record_id(&partition, expected[0].id()),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        let change = tokio::time::timeout(Duration::from_secs(10), resumed.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(change, AuditLiveChange::RetainedRangeChanged));
        assert!(
            db.b.audit_filtered_block_records(&scope, &blocks[0], &filter)
                .await
                .is_err(),
            "a missing member invalidates an export even when the SQL filter would exclude it"
        );
    })
    .await
    .expect("audit LIVE, summary and replay qualification exceeded 90 seconds");
}
