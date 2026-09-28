#[path = "support/reads.rs"]
mod fixture;
#[path = "../../../testing/fixtures/store.rs"]
mod store;

use fixture::{create, owner, update};
use std::time::Duration;
use veoveo_optimization_mcp::{
    contract::{OptimizationCollection, OptimizationCollectionUri},
    reads::OptimizationReads,
};
use veoveo_task_runtime::TaskRuntime;

#[tokio::test]
async fn catalogs_and_completion_select_authorized_rows_before_limits() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = store::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "optimization", "writer");
        let runtime = TaskRuntime::new(db.b.clone(), "optimization", "reader");
        let reads = OptimizationReads::new(&runtime).unwrap();
        let caller = owner(Some("tenant-a"), "owner", "route-plan", &["mission"]);
        let denied = owner(Some("tenant-a"), "other", "route-plan", &["mission"]);
        for number in 1..=105 {
            let row = create(&writer, &denied, number).await;
            update(
                &writer,
                row.task,
                "UPDATE ONLY $task SET request.input.input = NONE RETURN NONE;",
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
            update(&writer, expected[100].task, "UPDATE ONLY $task SET request.owner.data_labels = ['mission', 'secret'] RETURN NONE;").await;
            assert!(reads.page(&caller, &after).await.unwrap().items.is_empty());
            let mut cleared = caller.clone();
            cleared.data_labels.insert("secret".into());
            assert_eq!(reads.page(&cleared, &after).await.unwrap().items.len(), 1);
            update(&writer, expected[100].task, "UPDATE ONLY $task SET request.owner.data_labels = ['mission'] RETURN NONE;").await;
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
        let db = store::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "optimization", "writer");
        let runtime = TaskRuntime::new(db.b.clone(), "optimization", "reader");
        let reads = OptimizationReads::new(&runtime).unwrap();
        let caller = owner(Some("tenant-a"), "owner", "route-plan", &["mission"]);
        for (number, mutation) in [
            "request.owner.principal_key = 'other'",
            "request.owner.profile = 'other'",
            "request.owner.tenant_key = NONE",
            "request.owner.data_labels = ['mission','secret']",
            "request.owner.authority.work_context = 'other'",
            "request.owner.authority.tenant = 'other'",
            "authority.context_key = 'other'",
            "work_context = work_context:other",
            "tenant = tenant:other",
            "owner = principal:other",
            "profile = profile:other",
            "server = mcp_server:other",
            "task_type = 'verify_solution'",
        ]
        .into_iter()
        .enumerate()
        {
            let row = create(&writer, &caller, number as u64 + 1).await;
            let sql = format!(
                "UPDATE ONLY $task SET {mutation}, request.input.input = NONE RETURN NONE;"
            );
            update(&writer, row.task, &sql).await;
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
        let db = store::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "optimization", "writer");
        let runtime = TaskRuntime::new(db.b.clone(), "optimization", "reader");
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
            other.authority.work_context = veoveo_types::WorkContextId::new("other").unwrap();
            assert!(reads.run(&other, &row.run).await.unwrap().is_none());
            update(
                &writer,
                row.task,
                "UPDATE ONLY $task SET request.owner.data_labels = ['secret'] RETURN NONE;",
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
            wrong.authority.tenant = veoveo_types::TenantId::new("other").unwrap();
            assert!(reads.problem(&wrong, &row.problem).await.is_err());
        }
    })
    .await
    .expect("Optimization tenant qualification exceeded 60 seconds");
}

#[tokio::test]
async fn solutions_require_success_and_selected_corruption_never_becomes_a_short_page() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db=store::TestDb::new().await;
        let writer=TaskRuntime::new(db.a.clone(),"optimization","writer");
        let runtime=TaskRuntime::new(db.b.clone(),"optimization","reader");
        let reads=OptimizationReads::new(&runtime).unwrap();
        let caller=owner(Some("tenant-a"),"owner","route-plan", &[]);
        for (number, mutation) in ["status = 'queued'","status = 'failed'","status = 'cancelled'","result.isError = true"].into_iter().enumerate() {
            let row=create(&writer,&caller,number as u64+1).await;
            update(&writer,row.task,&format!("UPDATE ONLY $task SET {mutation} RETURN NONE;")).await;
            assert!(reads.solution(&caller,&row.solution).await.unwrap().is_none());
            assert!(reads.complete_solutions(&caller,row.solution.as_str(),100).await.unwrap().values.is_empty());
            assert!(reads.page(&caller,&OptimizationCollectionUri::new(OptimizationCollection::Solutions,None).unwrap()).await.unwrap().items.is_empty());
        }
        let row=create(&writer,&caller,20).await;
        update(&writer, row.task, "UPDATE ONLY $task SET result.isError = 'false' RETURN NONE;").await;
        assert!(reads.run(&caller, &row.run).await.is_err());
        assert!(reads.solution(&caller, &row.solution).await.unwrap().is_none());
        update(&writer, row.task, "UPDATE ONLY $task SET result.isError = false RETURN NONE;").await;
        update(&writer,row.task,"UPDATE ONLY $task SET result.structuredContent.run_uri = 'optimization://run/run-0195dabe-7777-7abc-8def-ffffffffffff' RETURN NONE;").await;
        assert!(reads.solution(&caller,&row.solution).await.is_err());
        update(&writer,row.task,"UPDATE ONLY $task SET request.input.common.problem_id = 'malformed' RETURN NONE;").await;
        assert!(reads.complete_problems(&caller,"malformed",100).await.is_err());
        assert!(reads.run(&caller,&row.run).await.is_err());
    }).await.expect("Optimization result qualification exceeded 60 seconds");
}
