//! Isolated replicas qualify lease contention, writer drain and sealer takeover.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
mod support;
use std::{num::NonZeroU32, sync::Arc, time::Duration};
use veoveo_audit::{integrity::AuditSigningKey, *};
use veoveo_platform_store::StoreError;

fn draft() -> AuditDraft {
    AuditDraft::builder(
        AuditRequest::background(),
        AuditTarget::WorkContext {
            tenant: "test-tenant".parse().unwrap(),
            context: "operations".parse().unwrap(),
        },
        AuditDetail::AccountChange {
            activity: AccountActivity::Update,
        },
        AuditOutcome::Succeeded,
        AuditReason::Accepted,
    )
    .build()
    .unwrap()
}
async fn state(health: &mut AuditHealth, expected: AuditHealthState) {
    tokio::time::timeout(Duration::from_secs(15), async {
        while health.state() != expected {
            health.changed().await.unwrap();
        }
    })
    .await
    .expect("audit replica did not reach its expected state");
}

#[tokio::test]
async fn replay_drains_unrelated_changes_before_the_export_deadline() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::with_backend(fixture::StoreBackend::RocksDb).await;
        db.a.client()
            .query("DEFINE TABLE audit_sealer_noise CHANGEFEED 7d;")
            .await
            .unwrap()
            .check()
            .unwrap();
        // Separate commits model recording traffic ahead of an audit marker.
        // A one-second delay per 32-entry replay page cannot meet the reader deadline.
        for index in 0..768_u32 {
            db.a.client()
                .query("CREATE type::record('audit_sealer_noise', $index) SET value = $index;")
                .bind(("index", index))
                .await
                .unwrap()
                .check()
                .unwrap();
        }
        let record = draft();
        db.a.append_audit_records(std::slice::from_ref(&record))
            .await
            .unwrap();
        let started = std::time::Instant::now();
        let service = AuditService::start(
            db.a.clone(),
            Arc::new(AuditSigningKey::from_seed(&[43; 32])),
            NonZeroU32::new(1).unwrap(),
            Default::default(),
        )
        .unwrap();
        let sealed = tokio::time::timeout(
            Duration::from_secs(10),
            db.b.audit_wait_sealed(
                &AuditReadScope::new(None, true),
                &AuditPartition::Installation,
                record.id(),
            ),
        )
        .await;
        let elapsed = started.elapsed();
        // Always stop the service before reporting the result.
        service.shutdown(Duration::from_secs(15)).await.unwrap();
        sealed
            .expect("unrelated feed pages delayed an export marker beyond ten seconds")
            .unwrap();
        println!(
            "{{\"unrelated_commits\":768,\"seal_millis\":{}}}",
            elapsed.as_millis()
        );
    })
    .await
    .expect("audit replay backlog qualification exceeded 60 seconds");
}
#[tokio::test]
async fn busy_replay_and_export_preserve_one_provider_acknowledgement() {
    tokio::time::timeout(Duration::from_secs(90), async {
        use axum::{Json, Router, extract::State, routing::post};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use veoveo_audit::export::{AuditExportConfig, AuditExporter, OtlpConfig};
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Request {
            resource_logs: Vec<Resource>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Resource {
            scope_logs: Vec<Scope>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Scope {
            log_records: Vec<serde::de::IgnoredAny>,
        }
        struct Server(tokio::task::JoinHandle<()>);
        impl Drop for Server {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let accepted = Arc::new(AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/logs", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let app = Router::new().route("/v1/logs", post(
            |State(accepted): State<Arc<AtomicUsize>>, Json(request): Json<Request>| async move {
                let count = request.resource_logs.into_iter().flat_map(|r| r.scope_logs)
                    .map(|s| s.log_records.len()).sum::<usize>();
                accepted.fetch_add(count, Ordering::SeqCst);
                ([("content-type", "application/json")], "{}")
            }
        )).with_state(accepted.clone());
        let mut server = Server(tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        }));
        let exports = AuditExportConfig {
            s3: None,
            otlp: Some(OtlpConfig {
                endpoint,
                allow_http: true,
                bearer_token_env: None,
            }),
        };
        let destination = AuditExporter::new(exports.clone())
            .unwrap()
            .destination_ids()
            .remove(0);
        let db = fixture::TestDb::with_backend(fixture::StoreBackend::RocksDb).await;
        db.a.client()
            .query("DEFINE TABLE audit_export_noise CHANGEFEED 7d;")
            .await
            .unwrap()
            .check()
            .unwrap();
        let service = AuditService::start(
            db.a.clone(),
            Arc::new(AuditSigningKey::from_seed(&[47; 32])),
            NonZeroU32::new(1).unwrap(),
            exports,
        )
        .unwrap();
        let mut records = Vec::new();
        // Continuous domain commits overlap LIVE sealing and HTTP acknowledgements.
        for index in 0..768_u32 {
            db.b.client()
                .query("CREATE type::record('audit_export_noise', $index) SET value = $index;")
                .bind(("index", index))
                .await
                .unwrap()
                .check()
                .unwrap();
            if index % 16 == 0 {
                let record = draft();
                db.b.append_audit_records(std::slice::from_ref(&record))
                    .await
                    .unwrap();
                records.push(record.id());
            }
        }
        let result = tokio::time::timeout(Duration::from_secs(30), async {
            db.b.audit_wait_sealed(
                &AuditReadScope::new(None, true),
                &AuditPartition::Installation,
                *records.last().unwrap(),
            )
            .await
            .unwrap();
            while db
                .b
                .audit_export_candidate(&destination)
                .await
                .unwrap()
                .is_some()
            {
                assert_ne!(service.health().state(), AuditHealthState::Failed);
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        service.shutdown(Duration::from_secs(15)).await.unwrap();
        server.0.abort();
        let _ = (&mut server.0).await;
        result.expect("sealing and export did not catch up within 30 seconds");
        assert_eq!(
            accepted.load(Ordering::SeqCst),
            records.len(),
            "a database receipt conflict must not duplicate a provider acknowledgement"
        );
    })
    .await
    .expect("busy replay/export qualification exceeded 90 seconds");
}

#[tokio::test]
async fn a_standby_seals_after_takeover_and_shutdown_drains_all_committed_pages() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::with_backend(fixture::StoreBackend::RocksDb).await;
        let lease =
            db.a.acquire_audit_seal_lease(uuid::Uuid::now_v7())
                .await
                .unwrap();
        assert!(matches!(
            db.b.acquire_audit_seal_lease(uuid::Uuid::now_v7()).await,
            Err(StoreError::AuditLeaseBusy)
        ));
        let key = Arc::new(AuditSigningKey::from_seed(&[7; 32]));
        let first = AuditService::start(
            db.a.clone(),
            key.clone(),
            NonZeroU32::new(1).unwrap(),
            Default::default(),
        )
        .unwrap();
        let mut first_health = first.health();
        state(&mut first_health, AuditHealthState::Standby).await;
        assert!(
            first_health.ready(),
            "a valid elected peer is a healthy standby state"
        );

        let records = (0..193).map(|_| draft()).collect::<Vec<_>>();
        for batch in records.chunks(64) {
            db.a.append_audit_records(batch).await.unwrap();
        }
        db.a.release_audit_seal_lease(&lease).await.unwrap();
        state(&mut first_health, AuditHealthState::Active).await;
        let second = AuditService::start(
            db.b.clone(),
            key,
            NonZeroU32::new(1).unwrap(),
            Default::default(),
        )
        .unwrap();
        let mut second_health = second.health();
        state(&mut second_health, AuditHealthState::Standby).await;

        let writer = AuditWriter::start(db.a.clone());
        let required_count = 16;
        futures::future::join_all((0..required_count).map(|_| {
            let record = draft();
            let writer = &writer;
            let reader = &db.b;
            async move {
                writer.record(record.clone()).await.unwrap();
                let mut query = AuditQuery::new(AuditPartition::Installation);
                query.trace = Some(record.request().trace_id.clone());
                let page = reader
                    .audit_page(&AuditReadScope::new(None, true), &query)
                    .await
                    .unwrap();
                assert_eq!(page.records.len(), 1, "acknowledgement requires commit");
                assert_eq!(page.records[0].draft, record);
            }
        }))
        .await;
        let completion = draft();
        writer.record_completion(completion.clone()).await;
        writer.shutdown(Duration::from_secs(10)).await.unwrap();
        assert!(matches!(
            writer.record(draft()).await,
            Err(AuditWriteError::Closed)
        ));
        first.shutdown(Duration::from_secs(15)).await.unwrap();
        let scope = AuditReadScope::new(None, true);
        let mut cursor = None;
        let mut count = 0;
        loop {
            let blocks =
                db.b.audit_blocks(&scope, &AuditPartition::Installation, cursor, 100)
                    .await
                    .unwrap();
            if blocks.is_empty() {
                break;
            }
            for block in blocks {
                count +=
                    db.b.audit_block_records(&scope, &block)
                        .await
                        .unwrap()
                        .len();
                cursor = Some(block.head.sequence);
            }
        }
        assert_eq!(
            count,
            records.len() + required_count + 1,
            "shutdown must seal every feed page, including queued completions"
        );
        state(&mut second_health, AuditHealthState::Active).await;
        let after_takeover = draft();
        db.a.append_audit_records(std::slice::from_ref(&after_takeover))
            .await
            .unwrap();
        tokio::time::timeout(
            Duration::from_secs(10),
            db.a.audit_wait_sealed(&scope, &AuditPartition::Installation, after_takeover.id()),
        )
        .await
        .unwrap()
        .unwrap();
        second.shutdown(Duration::from_secs(15)).await.unwrap();
    })
    .await
    .expect("audit service qualification exceeded 90 seconds");
}

#[tokio::test]
async fn shutdown_reports_a_failed_seal_and_preserves_the_unsealed_record() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::with_backend(fixture::StoreBackend::RocksDb).await;
        db.a.client()
            .query(
                "DEFINE EVENT fixture_seal_failure ON TABLE audit_block
            WHEN $event = 'CREATE' THEN { THROW 'fixture_seal_failure'; };",
            )
            .await
            .unwrap()
            .check()
            .unwrap();
        let service = AuditService::start(
            db.a.clone(),
            Arc::new(AuditSigningKey::from_seed(&[29; 32])),
            NonZeroU32::new(1).unwrap(),
            Default::default(),
        )
        .unwrap();
        state(&mut service.health(), AuditHealthState::Active).await;
        let record = draft();
        db.a.append_audit_records(std::slice::from_ref(&record))
            .await
            .unwrap();
        assert!(
            matches!(
                service.shutdown(Duration::from_secs(15)).await,
                Err(AuditServiceError::Seal(_))
            ),
            "a failed drain must not report a successful shutdown"
        );
        let scope = AuditReadScope::new(None, true);
        let page =
            db.b.audit_page(&scope, &AuditQuery::new(AuditPartition::Installation))
                .await
                .unwrap();
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.records[0].draft.id(), record.id());
        assert!(
            db.b.audit_blocks(&scope, &AuditPartition::Installation, None, 10)
                .await
                .unwrap()
                .is_empty()
        );
    })
    .await
    .expect("failed shutdown qualification exceeded 90 seconds");
}

#[tokio::test]
async fn export_intent_receipts_and_rejections_fence_retention_across_replicas() {
    tokio::time::timeout(Duration::from_secs(90), async {
        use veoveo_types::Sha256Digest;
        let db = fixture::TestDb::with_backend(fixture::StoreBackend::RocksDb).await;
        db.a.append_audit_records(&[draft()]).await.unwrap();
        let mut lease = db.a.acquire_audit_seal_lease(uuid::Uuid::now_v7()).await.unwrap();
        let page = support::committed_records(&db.a, &mut lease).await;
        let records = page.records.iter().map(|(_, row)| row.clone()).collect::<Vec<_>>();
        let members = page.records.iter().map(|(stamp, row)| AuditBlockMember { id: row.draft.id(), versionstamp: *stamp }).collect();
        let key = AuditSigningKey::from_seed(&[19; 32]);
        let block = key.seal(AuditPartition::Installation, None, members, &records, chrono::Utc::now()).unwrap();
        db.a.commit_audit_blocks(&lease, page.next, std::slice::from_ref(&block)).await.unwrap();
        let first = AuditDestinationId::from_configuration_hash(Sha256Digest::from_bytes([1;32]));
        let second = AuditDestinationId::from_configuration_hash(Sha256Digest::from_bytes([2;32]));
        let destinations = vec![first.clone(), second.clone()];
        let payload = AuditExportPayload { content: Sha256Digest::from_bytes([3;32]), seal: Sha256Digest::from_bytes([4;32]) };
        let cutoff = chrono::Utc::now() + chrono::TimeDelta::days(1);
        assert!(matches!(db.a.complete_audit_export(&lease, &first, &block, &payload).await, Err(StoreError::AuditIntegrity)), "a receipt requires committed intent");
        // Multiple connections race on the lease, intent and cursor fences.
        // Every confirmed-abort retry must preserve one immutable intent.
        let prepares = (0..16).map(|index| {
            let store = if index % 2 == 0 { &db.a } else { &db.b };
            store.prepare_audit_export(&lease, &first, &block, &payload)
        });
        for result in futures::future::join_all(prepares).await { result.unwrap(); }
        assert!(db.b.audit_retention_candidates(cutoff, &destinations).await.unwrap().is_empty());
        let different = AuditExportPayload { content: Sha256Digest::from_bytes([5;32]), ..payload.clone() };
        assert!(matches!(db.b.prepare_audit_export(&lease, &first, &block, &different).await, Err(StoreError::AuditIntegrity)));
        let receipts = (0..16).map(|index| {
            let store = if index % 2 == 0 { &db.a } else { &db.b };
            store.complete_audit_export(&lease, &first, &block, &payload)
        });
        for result in futures::future::join_all(receipts).await { result.unwrap(); }
        assert!(db.b.audit_export_candidate(&first).await.unwrap().is_none());
        assert!(db.b.audit_export_candidate(&second).await.unwrap().is_some());
        assert!(matches!(db.b.retain_audit_block(&lease, &block, cutoff, &destinations).await, Err(StoreError::AuditRetentionNotAdmitted)), "every configured destination must acknowledge");
        db.a.prepare_audit_export(&lease, &second, &block, &payload).await.unwrap();
        db.a.reject_audit_export(&lease, &second, &block, &payload, AuditExportRejection::PartialAcceptance).await.unwrap();
        assert!(matches!(db.b.prepare_audit_export(&lease, &second, &block, &payload).await, Err(StoreError::AuditExportRejected)), "another replica cannot retry a recorded partial rejection");
        db.a.release_audit_seal_lease(&lease).await.unwrap();
        let next_lease = db.b.acquire_audit_seal_lease(uuid::Uuid::now_v7()).await.unwrap();
        assert!(matches!(db.a.retain_audit_block(&lease, &block, cutoff, std::slice::from_ref(&first)).await, Err(StoreError::AuditLeaseLost)));
        // Both reference cascades participate in the deleting transaction. A
        // later abort must preserve seal membership and every export intent.
        assert!(db.a.client().query("BEGIN; DELETE audit_record; DELETE audit_block; THROW 'qualification_rollback'; COMMIT;").await.unwrap().check().is_err());
        let mut retained = db.b.client().query("RETURN array::len(SELECT id FROM audit_record); RETURN array::len(SELECT id FROM audit_record_seal); RETURN array::len(SELECT id FROM audit_export_delivery); RETURN array::len(SELECT id FROM audit_block);").await.unwrap().check().unwrap();
        for (index, count) in [1_u64, 1, 2, 1].into_iter().enumerate() {
            assert_eq!(retained.take::<Option<u64>>(index).unwrap(), Some(count));
        }
        // The operator selects the destination that delivered successfully. The
        // retired signed anchor and its delivery cursor outlive record deletion.
        db.b.retain_audit_block(&next_lease, &block, cutoff, std::slice::from_ref(&first)).await.unwrap();
        let mut counts = db.a.client().query("RETURN array::len(SELECT id FROM audit_record); RETURN array::len(SELECT id FROM audit_export_delivery); RETURN array::len(SELECT id FROM audit_export_cursor); RETURN array::len(SELECT id FROM audit_retention_anchor); RETURN array::len(SELECT id FROM audit_record_seal);").await.unwrap().check().unwrap();
        assert_eq!(counts.take::<Option<u64>>(0).unwrap(), Some(0));
        assert_eq!(counts.take::<Option<u64>>(1).unwrap(), Some(0));
        assert_eq!(counts.take::<Option<u64>>(2).unwrap(), Some(1));
        assert_eq!(counts.take::<Option<u64>>(3).unwrap(), Some(1));
        assert_eq!(counts.take::<Option<u64>>(4).unwrap(), Some(0));
    }).await.expect("export retention qualification exceeded 90 seconds");
}
