#[path = "support/reads.rs"]
mod fixture;
use store::module_lanes;
#[path = "../../../testing/fixtures/store.rs"]
mod store;

async fn install(db: &store::TestDb) {
    let setup = veoveo_optimization_mcp::schema::module_setup(
        module_lanes::execution("optimization").unwrap(),
    )
    .unwrap();
    module_lanes::install(&db.a, vec![setup]).await.unwrap();
}

use fixture::{create, owner, runtime, update};
use std::time::Duration;
use veoveo_optimization_mcp::{
    contract::{OptimizationCollection, OptimizationCollectionUri},
    reads::OptimizationReads,
};
use veoveo_task_runtime::TaskRuntime;

async fn rejected_task_shape(runtime: &TaskRuntime, task: veoveo_types::TaskId, statement: &str) {
    use surrealdb::types::Value;
    let id = veoveo_platform_store::task_record_id(task);
    let before: Option<Value> = runtime
        .platform_store()
        .client()
        .select(id.clone())
        .await
        .unwrap();
    let error = runtime
        .platform_store()
        .client()
        .query(statement)
        .bind(("task", id.clone()))
        .await
        .unwrap()
        .check()
        .unwrap_err();
    assert!(
        error.to_string().contains("field"),
        "unexpected schema error: {error}"
    );
    let after: Option<Value> = runtime
        .platform_store()
        .client()
        .select(id.clone())
        .await
        .unwrap();
    assert_eq!(after, before, "rejected control mutation changed Task");
    let _: Option<Value> = runtime.platform_store().client().delete(id).await.unwrap();
}

#[tokio::test]
async fn catalogs_and_completion_select_authorized_rows_before_limits() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = store::TestDb::with_modules(vec![veoveo_optimization_mcp::schema::module_setup(store::module_lanes::execution("optimization").unwrap()).unwrap()]).await;
        install(&db).await;
        let writer = runtime(db.a.clone(), "writer");
        let runtime = runtime(db.b.clone(), "reader");
        let reads = OptimizationReads::new(&runtime).unwrap();
        let caller = owner(Some("tenant-a"), "owner", "route-plan", &["mission"]);
        let denied = owner(Some("tenant-a"), "other", "route-plan", &["mission"]);
        for number in 1..=105 {
            let row = create(&writer, &denied, number).await;
            update(
                &writer,
                row.task,
                include_str!("queries/reads/catalogs_and_completion_select_authorized_rows_before_limits.surql"),
            )
            .await;
        }
        let mut expected = Vec::new();
        for number in 200..=300 {
            expected.push(create(&writer, &caller, number).await);
        }
        for collection in [
            OptimizationCollection::Problems,
            OptimizationCollection::Runs,
            OptimizationCollection::Solutions,
        ] {
            let root = OptimizationCollectionUri::new(collection, None).unwrap();
            let first = reads.page(&caller, &root).await.unwrap();
            assert_eq!(
                first
                    .items
                    .iter()
                    .map(|row| row.snapshot.task_id)
                    .collect::<Vec<_>>(),
                expected[..100]
                    .iter()
                    .map(|row| row.task)
                    .collect::<Vec<_>>()
            );
            let after = OptimizationCollectionUri::new(collection, first.next_cursor).unwrap();
            let last = reads.page(&caller, &after).await.unwrap();
            assert_eq!(last.items.len(), 1);
            assert_eq!(last.items[0].snapshot.task_id, expected[100].task);
            assert!(last.next_cursor.is_none());
            update(&writer, expected[100].task, include_str!("queries/reads/catalogs_and_completion_select_authorized_rows_before_limits_2.surql")).await;
            assert!(reads.page(&caller, &after).await.unwrap().items.is_empty());
            let mut cleared = caller.clone();
            cleared.data_labels.insert("secret".into());
            assert_eq!(reads.page(&cleared, &after).await.unwrap().items.len(), 1);
            update(&writer, expected[100].task, include_str!("queries/reads/catalogs_and_completion_select_authorized_rows_before_limits_3.surql")).await;
        }
        let problems = reads
            .complete_problems(&caller, "problem-", 100)
            .await
            .unwrap();
        assert_eq!(problems.values.len(), 100);
        assert!(problems.has_more);
        let runs = reads.complete_runs(&caller, "RUN-", 100).await.unwrap();
        assert_eq!(runs.values.len(), 100);
        assert!(runs.has_more);
        let solutions = reads
            .complete_solutions(&caller, "solution-", 100)
            .await
            .unwrap();
        assert_eq!(solutions.values.len(), 100);
        assert!(solutions.has_more);
        let last = &expected[100];
        assert_eq!(
            reads
                .problem(&caller, &last.problem)
                .await
                .unwrap()
                .unwrap()
                .snapshot
                .task_id,
            last.task
        );
        assert_eq!(
            reads
                .run(&caller, &last.run)
                .await
                .unwrap()
                .unwrap()
                .snapshot
                .task_id,
            last.task
        );
        assert_eq!(
            reads
                .solution(&caller, &last.solution)
                .await
                .unwrap()
                .unwrap()
                .snapshot
                .task_id,
            last.task
        );
        assert_eq!(
            reads
                .complete_problems(&caller, last.problem.as_str(), 1)
                .await
                .unwrap()
                .values
                .as_slice(),
            std::slice::from_ref(&last.problem)
        );
        assert!(reads.complete_runs(&caller, "run", 0).await.is_err());
        assert!(reads.complete_runs(&caller, "run", 101).await.is_err());
        let foreign = TaskRuntime::new(db.b.clone(), "media", "foreign");
        assert!(OptimizationReads::new(&foreign).is_err());
    })
    .await
    .expect("Optimization catalog qualification exceeded 90 seconds");
}

#[tokio::test]
async fn ownership_envelope_and_context_must_agree_before_any_decoding() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = store::TestDb::with_modules(vec![veoveo_optimization_mcp::schema::module_setup(store::module_lanes::execution("optimization").unwrap()).unwrap()]).await;
        install(&db).await;
        let writer = runtime(db.a.clone(), "writer");
        let runtime = runtime(db.b.clone(), "reader");
        let reads = OptimizationReads::new(&runtime).unwrap();
        let caller = owner(Some("tenant-a"), "owner", "route-plan", &["mission"]);
        for (number, (mutation, statement)) in [
("owner_context.principal_key = 'other'", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_1.surql")),
("owner_context.profile = 'other'", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_2.surql")),
("owner_context.tenant_key = NONE", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_3.surql")),
("owner_context.data_labels = ['mission','secret']", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_4.surql")),
("owner_context.data_labels = NONE", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_5.surql")),
("owner_context.data_labels = 'mission'", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_6.surql")),
("owner_context.data_labels = ['mission', NONE]", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_7.surql")),
("owner_context.authority.work_context = 'other'", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_8.surql")),
("owner_context.authority.tenant = 'other'", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_9.surql")),
("authority.context_key = 'other'", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_10.surql")),
("work_context = work_context:other", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_11.surql")),
("tenant = tenant:other", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_12.surql")),
("owner = principal:other", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_13.surql")),
("profile = profile:other", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_14.surql")),
("server = mcp_server:other", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_15.surql")),
("task_type = 'verify_solution'", include_str!("queries/reads/ownership_envelope_and_context_must_agree_before_any_decoding_variant_16.surql"))
]
        .into_iter()
        .enumerate()
        {
            let row = create(&writer, &caller, number as u64 + 1).await;
            if matches!(number, 4..=6) {
                rejected_task_shape(&writer, row.task, statement).await;
                continue;
            }
            update(&writer, row.task, statement).await;
            assert!(
                reads
                    .problem(&caller, &row.problem)
                    .await
                    .unwrap()
                    .is_none(),
                "accepted {mutation}"
            );
            assert!(
                reads.run(&caller, &row.run).await.unwrap().is_none(),
                "accepted {mutation}"
            );
            assert!(
                reads
                    .solution(&caller, &row.solution)
                    .await
                    .unwrap()
                    .is_none(),
                "accepted {mutation}"
            );
            for collection in [
                OptimizationCollection::Problems,
                OptimizationCollection::Runs,
                OptimizationCollection::Solutions,
            ] {
                assert!(
                    reads
                        .page(
                            &caller,
                            &OptimizationCollectionUri::new(collection, None).unwrap()
                        )
                        .await
                        .unwrap()
                        .items
                        .is_empty(),
                    "page accepted {mutation}"
                );
            }
            assert!(
                reads
                    .complete_problems(&caller, "", 100)
                    .await
                    .unwrap()
                    .values
                    .is_empty(),
                "completion accepted {mutation}"
            );
            assert!(
                reads
                    .complete_runs(&caller, "", 100)
                    .await
                    .unwrap()
                    .values
                    .is_empty()
            );
            assert!(
                reads
                    .complete_solutions(&caller, "", 100)
                    .await
                    .unwrap()
                    .values
                    .is_empty()
            );
        }
    })
    .await
    .expect("Optimization ownership qualification exceeded 90 seconds");
}

#[tokio::test]
async fn optional_tenant_and_current_context_clearance_remain_distinct() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = store::TestDb::with_modules(vec![veoveo_optimization_mcp::schema::module_setup(store::module_lanes::execution("optimization").unwrap()).unwrap()]).await;
        install(&db).await;
        let writer = runtime(db.a.clone(), "writer");
        let runtime = runtime(db.b.clone(), "reader");
        let reads = OptimizationReads::new(&runtime).unwrap();
        let implicit = owner(None, "owner", "route-plan", &[]);
        let explicit = owner(Some("installation"), "owner", "route-plan", &[]);
        for (number, (allowed, denied)) in [(&implicit, &explicit), (&explicit, &implicit)]
            .into_iter()
            .enumerate()
        {
            let row = create(&writer, allowed, number as u64 + 1).await;
            assert!(reads.problem(denied, &row.problem).await.unwrap().is_none());
            assert!(
                reads
                    .problem(allowed, &row.problem)
                    .await
                    .unwrap()
                    .is_some()
            );
            let mut other = allowed.clone();
            other.authority.work_context = veoveo_types::WorkContextId::parse("other").unwrap();
            assert!(reads.run(&other, &row.run).await.unwrap().is_none());
            update(
                &writer,
                row.task,
                include_str!("queries/reads/optional_tenant_and_current_context_clearance_remain_distinct.surql"),
            )
            .await;
            assert!(
                reads
                    .solution(allowed, &row.solution)
                    .await
                    .unwrap()
                    .is_none()
            );
            let mut cleared = allowed.clone();
            cleared.data_labels.insert("secret".into());
            assert!(
                reads
                    .solution(&cleared, &row.solution)
                    .await
                    .unwrap()
                    .is_some()
            );
            let mut wrong = cleared;
            wrong.authority.tenant = veoveo_types::TenantId::parse("other").unwrap();
            assert!(reads.problem(&wrong, &row.problem).await.is_err());
        }
    })
    .await
    .expect("Optimization tenant qualification exceeded 60 seconds");
}

#[tokio::test]
async fn solutions_require_success_and_selected_corruption_never_becomes_a_short_page() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db=store::TestDb::with_modules(vec![veoveo_optimization_mcp::schema::module_setup(store::module_lanes::execution("optimization").unwrap()).unwrap()]).await;
        install(&db).await;
        let writer=runtime(db.a.clone(), "writer");
        let runtime=runtime(db.b.clone(), "reader");
        let reads=OptimizationReads::new(&runtime).unwrap();
        let caller=owner(Some("tenant-a"),"owner","route-plan", &[]);
        for (number, (_mutation, statement)) in [
("status = 'queued'", include_str!("queries/reads/solutions_require_success_and_selected_corruption_never_becomes_a_short_page_variant_1.surql")),
("status = 'failed'", include_str!("queries/reads/solutions_require_success_and_selected_corruption_never_becomes_a_short_page_variant_2.surql")),
("status = 'cancelled'", include_str!("queries/reads/solutions_require_success_and_selected_corruption_never_becomes_a_short_page_variant_3.surql")),
("result.payload.isError = true", include_str!("queries/reads/solutions_require_success_and_selected_corruption_never_becomes_a_short_page_variant_4.surql"))
].into_iter().enumerate() {
            let row=create(&writer,&caller,number as u64+1).await;
            // The nonsucceeded fixtures clear the Task product address required
            // by the shared schema. Their stale successful catalog settlement
            // and result bytes remain, requiring SQL to exclude them before
            // hydration would report a catalog/Task integrity mismatch.
            update(&writer,row.task,statement).await;
            if number < 3 {
                assert!(reads.solution(&caller,&row.solution).await.unwrap().is_none());
                assert!(reads.complete_solutions(&caller,row.solution.as_str(),100).await.unwrap().values.is_empty());
                assert!(reads.page(&caller,&OptimizationCollectionUri::new(OptimizationCollection::Solutions,None).unwrap()).await.unwrap().items.is_empty());
            } else {
                // A SQL-authorized successful Task reaches checked owner hydration.
                // An error-marked retained result is an integrity error.
                assert!(reads.solution(&caller,&row.solution).await.is_err());
                assert!(reads.complete_solutions(&caller,row.solution.as_str(),100).await.is_err());
                assert!(reads.page(&caller,&OptimizationCollectionUri::new(OptimizationCollection::Solutions,None).unwrap()).await.is_err());
            }
        }
        let row=create(&writer,&caller,20).await;
        update(&writer, row.task, include_str!("queries/reads/solutions_require_success_and_selected_corruption_never_becomes_a_short_page_2.surql")).await;
        assert!(reads.run(&caller, &row.run).await.is_err());
        assert!(reads.solution(&caller, &row.solution).await.is_err());
        update(&writer, row.task, include_str!("queries/reads/solutions_require_success_and_selected_corruption_never_becomes_a_short_page_3.surql")).await;
        update(&writer,row.task,include_str!("queries/reads/solutions_require_success_and_selected_corruption_never_becomes_a_short_page_4.surql")).await;
        assert!(reads.solution(&caller,&row.solution).await.is_err());
        update(&writer,row.task,include_str!("queries/reads/solutions_require_success_and_selected_corruption_never_becomes_a_short_page_5.surql")).await;
        assert!(reads.complete_problems(&caller,row.problem.as_str(),100).await.is_err());
        assert!(reads.run(&caller,&row.run).await.is_err());
        let wrong_kind = create(&writer, &caller, 21).await;
        update(&writer, wrong_kind.task, include_str!("queries/reads/corrupt_request_kind.surql")).await;
        assert!(reads.problem(&caller, &wrong_kind.problem).await.is_err());
        assert!(reads.solution(&caller, &wrong_kind.solution).await.is_err());
        assert!(reads.complete_problems(&caller, wrong_kind.problem.as_str(), 100).await.is_err());
    }).await.expect("Optimization result qualification exceeded 60 seconds");
}

#[tokio::test]
async fn record_shaped_payloads_cannot_dereference_foreign_policy_or_result_rows() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = store::TestDb::with_modules(vec![veoveo_optimization_mcp::schema::module_setup(store::module_lanes::execution("optimization").unwrap()).unwrap()]).await;
        install(&db).await;
        let writer = runtime(db.a.clone(), "writer");
        let reader = runtime(db.b.clone(), "reader");
        let reads = OptimizationReads::new(&reader).unwrap();
        let caller = owner(Some("tenant-a"), "owner", "route-plan", &["mission"]);
        let row = create(&writer, &caller, 1).await;
        let snapshot = writer.get(row.task).await.unwrap().unwrap();
        let result = snapshot.result.clone().unwrap();
        db.a.client().query(include_str!("queries/reads/record_shaped_payloads_cannot_dereference_foreign_policy_or_result_rows.surql"))
            .bind(("owner", veoveo_platform_store::OpenObject::new(serde_json::to_value(&caller).unwrap().as_object().unwrap().clone().into_iter().collect())))
            .bind(("authority", veoveo_platform_store::OpenObject::new(serde_json::to_value(&caller.authority).unwrap().as_object().unwrap().clone().into_iter().collect())))
            .bind(("input", veoveo_platform_store::OpenObject::new(snapshot.request.as_object().unwrap().clone().into_iter().collect())))
            .bind(("result", veoveo_platform_store::OpenObject::new(result.as_object().unwrap().clone().into_iter().collect())))
            .bind(("output", veoveo_platform_store::OpenObject::new(result["structuredContent"].as_object().unwrap().clone().into_iter().collect())))
            .await.unwrap().check().unwrap();
        for (number, (mutation, statement)) in [
("owner_context = record_guard:owner", include_str!("queries/reads/record_shaped_payloads_cannot_dereference_foreign_policy_or_result_rows_2_variant_1.surql")),
("owner_context.authority = record_guard:authority", include_str!("queries/reads/record_shaped_payloads_cannot_dereference_foreign_policy_or_result_rows_2_variant_2.surql")),
("owner_context.principal_key = record_guard:owner", include_str!("queries/reads/record_shaped_payloads_cannot_dereference_foreign_policy_or_result_rows_2_variant_3.surql")),
("owner_context.profile = record_guard:owner", include_str!("queries/reads/record_shaped_payloads_cannot_dereference_foreign_policy_or_result_rows_2_variant_4.surql")),
("owner_context.authority.work_context = record_guard:authority", include_str!("queries/reads/record_shaped_payloads_cannot_dereference_foreign_policy_or_result_rows_2_variant_5.surql")),
("request.input = record_guard:input", include_str!("queries/reads/record_shaped_payloads_cannot_dereference_foreign_policy_or_result_rows_2_variant_6.surql"))
].into_iter().enumerate() {
            let row = create(&writer, &caller, number as u64 + 10).await;
            if number < 5 {
                rejected_task_shape(&writer, row.task, statement).await;
                continue;
            }
            update(&writer, row.task, statement).await;
            assert!(reads.run(&caller, &row.run).await.is_err(), "accepted {mutation}");
            assert!(reads.solution(&caller, &row.solution).await.is_err(), "accepted {mutation}");
        }
        for (number, (_mutation, statement)) in [
("result.payload = record_guard:result", include_str!("queries/reads/record_shaped_payloads_cannot_dereference_foreign_policy_or_result_rows_3_variant_1.surql")),
("result.payload.structuredContent = record_guard:output", include_str!("queries/reads/record_shaped_payloads_cannot_dereference_foreign_policy_or_result_rows_3_variant_2.surql"))
].into_iter().enumerate() {
            let row = create(&writer, &caller, number as u64 + 30).await;
            update(&writer, row.task, statement).await;
            assert!(reads.solution(&caller, &row.solution).await.is_err());
            assert!(reads.run(&caller, &row.run).await.is_err());
        }
        for (mutation, statement) in [
("request = record_guard:owner", include_str!("queries/reads/record_shaped_payloads_cannot_dereference_foreign_policy_or_result_rows_4_variant_1.surql")),
("authority = record_guard:authority", include_str!("queries/reads/record_shaped_payloads_cannot_dereference_foreign_policy_or_result_rows_4_variant_2.surql"))
] {
            let result = db.a.client().query(statement)
                .bind(("task", veoveo_platform_store::task_record_id(row.task))).await.unwrap().check();
            assert!(result.is_err(), "schema admitted {mutation}");
        }
        let page = OptimizationCollectionUri::new(OptimizationCollection::Solutions, None).unwrap();
        assert!(reads.page(&caller, &page).await.is_err());
    }).await.expect("Optimization opaque record guard qualification exceeded 90 seconds");
}

#[tokio::test]
async fn malformed_canonical_catalog_and_present_wrong_uri_fail_selected_reads() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = store::TestDb::with_modules(vec![veoveo_optimization_mcp::schema::module_setup(store::module_lanes::execution("optimization").unwrap()).unwrap()]).await;
        install(&db).await;
        let writer = runtime(db.a.clone(), "writer");
        let reader = runtime(db.b.clone(), "reader");
        let reads = OptimizationReads::new(&reader).unwrap();
        let caller = owner(Some("tenant-a"), "owner", "route-plan", &[]);
        let row = create(&writer, &caller, 1).await;
        update(
            &writer,
            row.task,
            include_str!("queries/reads/malformed_canonical_catalog_and_present_wrong_uri_fail_selected_reads.surql"),
        )
        .await;
        assert!(reads.solution(&caller, &row.solution).await.is_err());
        assert!(
            reads
                .complete_solutions(&caller, row.solution.as_str(), 100)
                .await
                .is_err()
        );
        assert!(
            reads
                .page(
                    &caller,
                    &OptimizationCollectionUri::new(OptimizationCollection::Solutions, None)
                        .unwrap()
                )
                .await
                .is_err()
        );
        let row = create(&writer, &caller, 2).await;
        db.a.client()
            .query(include_str!("queries/reads/malformed_canonical_catalog_and_present_wrong_uri_fail_selected_reads_2.surql"))
            .bind((
                "row",
                surrealdb::types::RecordId::new("optimization_task", row.task.to_string()),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            reads
                .complete_problems(&caller, "malformed", 100)
                .await
                .is_err()
        );
        assert!(reads.run(&caller, &row.run).await.is_err());
        let first = create(&writer, &caller, 3).await;
        let second = create(&writer, &caller, 4).await;
        db.a.client()
            .query(include_str!("queries/reads/malformed_canonical_catalog_and_present_wrong_uri_fail_selected_reads_3.surql"))
            .bind((
                "row",
                surrealdb::types::RecordId::new("optimization_task", second.task.to_string()),
            ))
            .bind(("problem", first.problem.to_string()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            reads
                .problem(&caller, &first.problem)
                .await
                .err()
                .expect("duplicate canonical catalog must fail")
                .to_string()
                .contains("duplicate canonical")
        );
    })
    .await
    .expect("Optimization canonical catalog corruption qualification exceeded 90 seconds");
}

use veoveo_platform_store::read_transaction;

#[tokio::test]
async fn native_read_snapshot_survives_settlement_revocation_and_parent_deletion() {
    tokio::time::timeout(Duration::from_secs(90), async {
        use veoveo_task_runtime::{CreateTask, TaskStatus, TaskTransition};
        let db = store::TestDb::with_modules(vec![veoveo_optimization_mcp::schema::module_setup(store::module_lanes::execution("optimization").unwrap()).unwrap()]).await;
        install(&db).await;
        let writer = runtime(db.a.clone(), "writer");
        let reader = runtime(db.b.clone(), "reader");
        let caller = owner(Some("tenant-a"), "owner", "route-plan", &[]);
        let seed = create(&writer, &caller, 1000).await;
        let seed_snapshot = writer.get(seed.task).await.unwrap().unwrap();
        let queued = veoveo_types::TaskId::new();
        let mut request = seed_snapshot.request.clone();
        request["common"]["artifact_write_capability"]["task_id"] =
            serde_json::to_value(queued).unwrap();
        db.a.client()
            .query(include_str!("queries/reads/native_read_snapshot_survives_settlement_revocation_and_parent_deletion.surql"))
            .bind(("task", veoveo_platform_store::task_record_id(seed.task)))
            .await
            .unwrap()
            .check()
            .unwrap();
        writer
            .create(CreateTask {
                task_id: queued,
                owner: caller.clone(),
                server: "optimization".into(),
                task_type: seed_snapshot.task_type.clone(),
                request,
                recovery_class: seed_snapshot.recovery_class,
                idempotency_key: None,
                ttl_ms: None,
                poll_interval_ms: None,
                retention_pins: Default::default(),
            })
            .await
            .unwrap();
        let deleted = create(&writer, &caller, 1001).await.task;
        let (selected, selected_rx) = tokio::sync::oneshot::channel();
        let (resume, resume_rx) = tokio::sync::oneshot::channel();
        let client = db.b.client().clone();
        let admitted = caller.clone();
        let read = tokio::spawn(async move {
            read_transaction::read(&client, move |transaction| {
                Box::pin(async move {
                    let mut response = transaction
                        .query(
                            include_str!("queries/reads/native_read_snapshot_survives_settlement_revocation_and_parent_deletion_2.surql"),
                        )
                        .bind((
                            "tasks",
                            vec![
                                veoveo_platform_store::task_record_id(queued),
                                veoveo_platform_store::task_record_id(deleted),
                            ],
                        ))
                        .await?
                        .check()?;
                    let rows: Vec<surrealdb::types::Value> = response.take(0)?;
                    assert_eq!(rows.len(), 2);
                    selected.send(()).unwrap();
                    resume_rx.await?;
                    Ok(reader
                        .for_owner(&admitted)
                        .in_work_context()?
                        .get_many_in(transaction, &[queued, deleted])
                        .await?)
                })
            })
            .await
        });
        selected_rx.await.unwrap();
        writer.claim(queued, Duration::from_secs(60)).await.unwrap();
        writer
            .transition(
                queued,
                TaskTransition::Succeeded {
                    message: "settled concurrently".into(),
                    result: seed_snapshot.result.unwrap(),
                    result_uri: seed_snapshot.result_uri,
                },
            )
            .await
            .unwrap();
        update(
            &writer,
            queued,
            include_str!("queries/reads/native_read_snapshot_survives_settlement_revocation_and_parent_deletion_3.surql"),
        )
        .await;
        db.a.client()
            .query(include_str!("queries/reads/native_read_snapshot_survives_settlement_revocation_and_parent_deletion_4.surql"))
            .bind(("task", veoveo_platform_store::task_record_id(deleted)))
            .await
            .unwrap()
            .check()
            .unwrap();
        resume.send(()).unwrap();
        let old = read.await.unwrap().unwrap();
        assert_eq!(old.len(), 2);
        assert_eq!(
            old.iter().find(|row| row.task_id == queued).unwrap().status,
            TaskStatus::Queued
        );
        assert_eq!(
            old.iter()
                .find(|row| row.task_id == deleted)
                .unwrap()
                .status,
            TaskStatus::Succeeded
        );
        let current = runtime(db.b.clone(), "current");
        assert!(
            current
                .for_owner(&caller)
                .in_work_context()
                .unwrap()
                .get(queued)
                .await
                .unwrap()
                .is_none()
        );
        assert!(current.get(deleted).await.unwrap().is_none());
    })
    .await
    .expect("Optimization native snapshot race qualification exceeded 90 seconds");
}

#[tokio::test]
async fn native_read_worker_finishes_cleanup_after_timeout_or_dropped_awaiter() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = store::TestDb::with_modules(vec![
            veoveo_optimization_mcp::schema::module_setup(
                store::module_lanes::execution("optimization").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        install(&db).await;
        let (ready, ready_rx) = tokio::sync::oneshot::channel();
        let (released, released_rx) = tokio::sync::oneshot::channel();
        struct Released(Option<tokio::sync::oneshot::Sender<()>>);
        impl Drop for Released {
            fn drop(&mut self) {
                if let Some(sender) = self.0.take() {
                    let _ = sender.send(());
                }
            }
        }
        let client = db.a.client().clone();
        let caller = tokio::spawn(async move {
            read_transaction::read(&client, move |transaction| {
                Box::pin(async move {
                    transaction
                        .query(include_str!("queries/reads/drop.surql"))
                        .await?
                        .check()?;
                    let _pending_operation = Released(Some(released));
                    ready.send(()).unwrap();
                    std::future::pending::<anyhow::Result<()>>().await
                })
            })
            .await
        });
        ready_rx.await.unwrap();
        caller.abort();
        // The pending operation must be dropped promptly after its awaiter disappears.
        // The timeout case below separately checks the worker's completed cancellation result.
        tokio::time::timeout(Duration::from_secs(5), released_rx)
            .await
            .unwrap()
            .unwrap();
        let result: anyhow::Result<()> = read_transaction::read(db.a.client(), |transaction| {
            Box::pin(async move {
                transaction
                    .query(include_str!("queries/reads/drop_2.surql"))
                    .await?
                    .check()?;
                std::future::pending().await
            })
        })
        .await;
        assert_eq!(
            result.unwrap_err().to_string(),
            "owned read transaction timed out"
        );
        let mut response =
            db.a.client()
                .query(include_str!("queries/reads/drop_3.surql"))
                .await
                .unwrap()
                .check()
                .unwrap();
        assert!(
            response
                .take::<Vec<surrealdb::types::Value>>(0)
                .unwrap()
                .is_empty()
        );
    })
    .await
    .expect("Optimization native read cancellation qualification exceeded 90 seconds");
}

#[tokio::test]
async fn kernel_selection_rejects_partial_context_triplets() {
    tokio::time::timeout(Duration::from_secs(60), async {
        use veoveo_platform_store::{
            RecordId, deterministic_principal_id, deterministic_tenant_id,
            deterministic_work_context_id,
        };
        use veoveo_types::TaskTypeDefinition;
        let db = store::TestDb::with_modules(vec![
            veoveo_optimization_mcp::schema::module_setup(
                store::module_lanes::execution("optimization").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        install(&db).await;
        let writer = runtime(db.a.clone(), "writer");
        let caller = owner(Some("tenant-a"), "owner", "route-plan", &[]);
        let row = create(&writer, &caller, 1).await;
        for missing in 0..4 {
            let mut response =
                db.a.client()
                    .query(include_str!(
                        "queries/reads/kernel_selection_rejects_partial_context_triplets.surql"
                    ))
                    .bind(("task", veoveo_platform_store::task_record_id(row.task)))
                    .bind(("server", RecordId::new("mcp_server", "optimization")))
                    .bind((
                        "tenant",
                        deterministic_tenant_id(caller.tenant_key())
                            .unwrap()
                            .record_id(),
                    ))
                    .bind((
                        "owner",
                        deterministic_principal_id(caller.tenant_key(), &caller.principal_key)
                            .unwrap()
                            .record_id(),
                    ))
                    .bind(("profile", RecordId::new("profile", caller.profile.clone())))
                    .bind(("principal_key", caller.principal_key.clone()))
                    .bind(("profile_key", caller.profile.clone()))
                    .bind(("tenant_key", caller.tenant_key.clone()))
                    .bind(("labels", caller.data_labels.clone()))
                    .bind((
                        "task_types",
                        Some(vec![
                            veoveo_optimization_mcp::contract::OptimizationTaskKind::SolveConvex
                                .name()
                                .to_string(),
                        ]),
                    ))
                    .bind((
                        "work_context",
                        (missing != 0).then(|| {
                            deterministic_work_context_id(
                                caller.tenant_key(),
                                caller.authority.work_context.as_str(),
                            )
                            .unwrap()
                            .record_id()
                        }),
                    ))
                    .bind((
                        "work_context_key",
                        (missing != 1).then(|| caller.authority.work_context.to_string()),
                    ))
                    .bind((
                        "authority_tenant",
                        (missing != 2).then(|| caller.authority.tenant.to_string()),
                    ))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            let selected: Option<surrealdb::types::Value> = response.take(0).unwrap();
            assert_eq!(
                selected.is_some(),
                missing == 3,
                "accepted partial context triplet {missing}"
            );
        }
    })
    .await
    .expect("Task context triplet qualification exceeded 60 seconds");
}
