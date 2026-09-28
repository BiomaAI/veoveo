#[path = "support/generation.rs"]
mod generation_fixture;
#[path = "../../../testing/fixtures/store.rs"]
mod store;
#[path = "../src/bin/server/subscriptions.rs"]
mod subscriptions;
use chrono::Utc;
use futures::StreamExt;
use serde_json::json;
use std::{collections::BTreeSet, time::Duration};
use veoveo_mcp_contract::{UsageKind, UsageRecord};
use veoveo_media_mcp::{
    contract::*,
    provider::Prediction,
    reads::MediaReads,
    state::{MediaProviderJob, MediaState},
    task_results,
};
use veoveo_platform_store::{
    OpenObject, ProviderJobId, ProviderJobRecord, ProviderJobState, task_record_id,
};
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskOwner, TaskRuntime, TaskSnapshot};
use veoveo_types::TaskId;

fn owner(tenant: Option<&str>, principal: &str, profile: &str, labels: &[&str]) -> TaskOwner {
    serde_json::from_value(json!({"principal_key":principal,"principal_kind":"service","issuer":"https://media.test","subject":principal,"profile":profile,"tenant_key":tenant,"data_labels":labels,
        "authority":{"work_context":"studio","tenant":tenant.unwrap_or("installation"),"membership":"contributor","policy_revision":"r1","output_policy":{"owner":{"kind":"principal","id":principal}},"provenance":{"mode":"automated"}}})).unwrap()
}
async fn create(
    tasks: &TaskRuntime,
    owner: &TaskOwner,
    number: u64,
    prediction_id: &str,
) -> (TaskSnapshot, MediaProviderJob) {
    let task_id: TaskId = format!("0195dabe-7777-7abc-8def-{number:012x}")
        .parse()
        .unwrap();
    let task = tasks
        .create(CreateTask {
            task_id,
            owner: owner.clone(),
            server: tasks.server().into(),
            task_type: const { veoveo_types::TaskTypeName::from_static("run") },
            request: json!({}),
            recovery_class: RecoveryClass::WebhookWait,
            idempotency_key: None,
            ttl_ms: None,
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap()
        .snapshot;
    let id = ProviderJobId::new();
    let prediction = Prediction {
        id: MediaPredictionId::new(prediction_id).unwrap(),
        model: "test/image".into(),
        outputs: vec!["https://provider.test/private-output".into()],
        urls: None,
        status: "completed".into(),
        created_at: Some(Utc::now()),
        error: None,
        execution_time: Some(10.),
        timings: None,
        input: None,
    };
    let job = MediaProviderJob {
        job_id: id,
        task_id,
        external_job_id: prediction.id.clone(),
        state: ProviderJobState::Succeeded,
        prediction: prediction.clone(),
        updated_at: Utc::now(),
    };
    let record = ProviderJobRecord {
        id: id.record_id(),
        tenant: veoveo_platform_store::deterministic_tenant_id(owner.tenant_key())
            .unwrap()
            .record_id(),
        task: task_record_id(task_id),
        provider: "media".into(),
        external_job_id: prediction.id.to_string(),
        state: job.state,
        provider_payload: OpenObject::new(
            serde_json::to_value(prediction)
                .unwrap()
                .as_object()
                .unwrap()
                .clone()
                .into_iter()
                .collect(),
        ),
        submitted_at: Utc::now(),
        updated_at: Utc::now(),
        completed_at: None,
    };
    tasks
        .platform_store()
        .client()
        .query("CREATE ONLY $job CONTENT $record RETURN NONE;")
        .bind(("job", id.record_id()))
        .bind(("record", record))
        .await
        .unwrap()
        .check()
        .unwrap();
    let state = MediaState::new(tasks.platform_store().clone());
    for (kind, name) in [
        (UsageKind::Estimate, "estimate"),
        (UsageKind::Actual, "actual"),
    ] {
        state
            .record_usage(
                &task,
                Some(&job),
                &UsageRecord {
                    task_id: task_id.to_string(),
                    source_id: Some(name.into()),
                    provider_job_id: Some(job.external_job_id.to_string()),
                    model_id: "test/image".into(),
                    kind,
                    quantity: Some(1.),
                    unit: Some("run".into()),
                    amount: None,
                    currency: None,
                    recorded_at: Utc::now(),
                    metadata: json!({}),
                },
            )
            .await
            .unwrap();
    }
    (task, job)
}

#[tokio::test]
async fn media_sql_pages_filter_denied_tasks_and_recheck_current_clearance() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = store::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "media", "writer");
        let reader = TaskRuntime::new(db.b.clone(), "media", "reader");
        let foreign = TaskRuntime::new(db.a.clone(), "other", "writer");
        assert!(MediaReads::new(&foreign).is_err());
        let reads = MediaReads::new(&reader).unwrap();
        let caller = owner(Some("tenant-a"), "owner", "operator", &["mission"]);
        let denied = [
            owner(Some("tenant-a"), "other", "operator", &[]),
            owner(Some("tenant-b"), "owner", "operator", &[]),
            owner(Some("tenant-a"), "owner", "observer", &[]),
            owner(Some("tenant-a"), "owner", "operator", &["secret"]),
        ];
        for number in 1..106 {
            let (task, job) = create(
                &writer, &denied[(number % 4) as usize], number, &format!("job-{number:05}"),
            ).await;
            db.a.client()
                .query("UPDATE ONLY $task SET request.input = NONE RETURN NONE; UPDATE ONLY $job SET provider_payload.model = NONE RETURN NONE;")
                .bind(("task", task_record_id(task.task_id)))
                .bind(("job", job.job_id.record_id()))
                .await.unwrap().check().unwrap();
        }
        create(&foreign, &caller, 106, "job-00106").await;
        let mut expected = Vec::new();
        for n in 200..301 {
            expected.push(create(&writer, &caller, n, &format!("job-{n:05}")).await);
        }
        let first = reads.usage_page(&caller, None).await.unwrap();
        assert_eq!(
            first.items().iter().map(MediaUsageEntry::task_id).collect::<Vec<_>>(),
            expected[..100].iter().map(|(task, _)| task.task_id).collect::<Vec<_>>()
        );
        let second = reads.usage_page(&caller, first.next_cursor()).await.unwrap();
        assert_eq!(second.items().len(), 1);
        assert!(second.next_cursor().is_none());
        let predictions = reads.predictions(&caller, None).await.unwrap();
        assert_eq!(predictions.items().len(), 100);
        let final_predictions = reads.predictions(&caller, predictions.next_cursor()).await.unwrap();
        assert_eq!(final_predictions.items().len(), 1);
        assert!(final_predictions.next_cursor().is_none());
        let usage = second.items()[0].usage_uri();
        let prediction = final_predictions.items()[0].prediction_uri();
        assert_eq!(reads.usage(&caller, usage).await.unwrap().len(), 2);
        let summary = reads.prediction(&caller, prediction).await.unwrap().unwrap();
        assert_eq!(summary.output_count, 1);
        assert!(serde_json::to_value(summary).unwrap().get("outputs").is_none());
        for denied in &denied[..3] {
            assert!(reads.usage(denied, usage).await.unwrap().is_empty());
            assert!(reads.prediction(denied, prediction).await.unwrap().is_none());
        }
        db.a.client()
            .query("UPDATE ONLY $task SET request.owner.data_labels = ['mission','secret'] RETURN NONE;")
            .bind(("task", task_record_id(usage.task_id())))
            .await.unwrap().check().unwrap();
        assert!(reads.usage_page(&caller, first.next_cursor()).await.unwrap().items().is_empty());
        assert!(reads.predictions(&caller, predictions.next_cursor()).await.unwrap().items().is_empty());
        assert!(!reads.task_visible(&caller, usage.task_id()).await.unwrap());
        let mut cleared = caller.clone();
        cleared.data_labels.insert("secret".into());
        assert_eq!(reads.usage(&cleared, usage).await.unwrap().len(), 2);
        assert!(reads.prediction(&cleared, prediction).await.unwrap().is_some());
        db.a.client()
            .query("UPDATE ONLY $job SET provider_payload.id = 'wrong-id' RETURN NONE;")
            .bind(("job", expected[100].1.job_id.record_id()))
            .await.unwrap().check().unwrap();
        assert!(reads.usage(&cleared, usage).await.unwrap().is_empty());
        assert!(reads.prediction(&cleared, prediction).await.unwrap().is_none());
        assert!(reads.usage_page(&cleared, first.next_cursor()).await.unwrap().items().is_empty());
        assert!(reads.predictions(&cleared, predictions.next_cursor()).await.unwrap().items().is_empty());
    }).await.expect("Media paging qualification exceeded 120 seconds");
}

#[tokio::test]
async fn external_id_collisions_and_optional_tenants_never_cross_authority() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = store::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "media", "writer");
        let reader = TaskRuntime::new(db.b.clone(), "media", "reader");
        let reads = MediaReads::new(&reader).unwrap();
        let foreign = owner(Some("foreign"), "owner", "operator", &[]);
        let caller = owner(Some("tenant-a"), "owner", "operator", &[]);
        let (foreign_task, _) = create(&writer, &foreign, 1, "shared/provider?id").await;
        let (task, job) = create(&writer, &caller, 2, "shared/provider?id").await;
        let uri = MediaPredictionUri::new(job.external_job_id.clone());
        assert!(reads.prediction(&caller, &uri).await.unwrap().is_some());
        assert_eq!(
            reads
                .predictions(&caller, None)
                .await
                .unwrap()
                .items()
                .len(),
            1
        );
        let state = MediaState::new(db.b.clone());
        assert_eq!(
            state
                .provider_job_for_task_prediction(task.task_id, &job.external_job_id)
                .await
                .unwrap()
                .unwrap()
                .job_id,
            job.job_id
        );
        assert!(
            state
                .has_actual_usage(task.task_id, &job.external_job_id)
                .await
                .unwrap()
        );
        let implicit = owner(None, "owner", "operator", &[]);
        let explicit = owner(Some("installation"), "owner", "operator", &[]);
        let (implicit_task, implicit_job) = create(&writer, &implicit, 3, "implicit").await;
        let (_, explicit_job) = create(&writer, &explicit, 4, "explicit").await;
        assert!(
            reads
                .prediction(
                    &explicit,
                    &MediaPredictionUri::new(implicit_job.external_job_id)
                )
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            reads
                .prediction(
                    &implicit,
                    &MediaPredictionUri::new(explicit_job.external_job_id)
                )
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            !reads
                .task_visible(&explicit, implicit_task.task_id)
                .await
                .unwrap()
        );
        let usage = MediaTaskUsageUri::new(task.task_id).unwrap();
        db.a.client()
            .query("UPDATE ONLY $job SET task = $foreign_task RETURN NONE;")
            .bind(("job", job.job_id.record_id()))
            .bind(("foreign_task", task_record_id(foreign_task.task_id)))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(reads.usage(&caller, &usage).await.unwrap().is_empty());
        assert!(reads.prediction(&caller, &uri).await.unwrap().is_none());
        assert!(
            !state
                .has_actual_usage(task.task_id, &job.external_job_id)
                .await
                .unwrap()
        );
        assert!(
            reads
                .usage_page(&caller, None)
                .await
                .unwrap()
                .items()
                .is_empty()
        );
    })
    .await
    .expect("Media parent qualification exceeded 60 seconds");
}

#[tokio::test]
async fn billing_pages_select_unsettled_terminal_jobs_before_limits() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = store::TestDb::new().await;
        let tasks = TaskRuntime::new(db.a.clone(), "media", "writer");
        let caller = owner(Some("tenant-a"), "owner", "operator", &[]);
        let state = MediaState::new(db.b.clone());
        // Settled rows sort first. Their malformed bodies must never be decoded.
        for n in 1..=101 {
            let (_, job) = create(&tasks, &caller, n, &format!("settled-{n}")).await;
            db.a.client().query("UPDATE ONLY $job SET provider_payload.model = NONE RETURN NONE;")
                .bind(("job", job.job_id.record_id())).await.unwrap().check().unwrap();
        }
        let mut expected = Vec::new();
        for n in 200..=300 {
            // The first unsettled job shares its external ID with a settled job
            // in another tenant. Billing settlement must follow the native job link.
            let (task, job) = create(&tasks, &owner(Some("tenant-b"), "owner", "operator", &[]), n,
                &if n == 200 { "settled-1".into() } else { format!("unsettled-{n}") }).await;
            db.a.client().query("DELETE media_usage WHERE task = $task AND kind = 'actual';")
                .bind(("task", task_record_id(task.task_id))).await.unwrap().check().unwrap();
            expected.push(job.job_id);
        }
        // Unsettled nonterminal, foreign-provider, inconsistent and orphaned jobs.
        for (n, mutation) in [
            (400, "provider_payload.status = 'processing', provider_payload.model = NONE"),
            (401, "provider = 'other', provider_payload.model = NONE"),
            (402, "tenant = tenant:missing, provider_payload.model = NONE"),
            (403, "provider_payload.id = 'inconsistent', provider_payload.model = NONE"),
        ] {
            let (task, job) = create(&tasks, &caller, n, &format!("excluded-{n}")).await;
            db.a.client().query(format!("DELETE media_usage WHERE task = $task; UPDATE ONLY $job SET {mutation} RETURN NONE;"))
                .bind(("task", task_record_id(task.task_id))).bind(("job", job.job_id.record_id())).await.unwrap().check().unwrap();
        }
        expected.sort();
        let first = state.billing_candidates(None).await.unwrap();
        assert_eq!(first.jobs.iter().map(|job| job.job_id).collect::<Vec<_>>(), expected[..100]);
        assert_eq!(first.next_job_id, Some(expected[99]));
        let last = state.billing_candidates(first.next_job_id).await.unwrap();
        assert_eq!(last.jobs.iter().map(|job| job.job_id).collect::<Vec<_>>(), expected[100..]);
        assert!(last.next_job_id.is_none());
        // A caller must not retain a stale candidate after its Task disappears.
        db.a.client().query("DELETE ONLY $task;").bind(("task", task_record_id(last.jobs[0].task_id)))
            .await.unwrap().check().unwrap();
        assert!(state.billing_candidates(first.next_job_id).await.unwrap().jobs.is_empty());
    }).await.expect("Media billing selection exceeded 90 seconds");
}

#[tokio::test]
async fn subscriptions_and_unlinked_estimates_follow_current_task_authority() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = store::TestDb::new().await;
        let tasks = TaskRuntime::new(db.a.clone(), "media", "writer");
        let reader = TaskRuntime::new(db.b.clone(), "media", "reader");
        let reads = MediaReads::new(&reader).unwrap();
        let caller = owner(Some("tenant-a"), "owner", "operator", &[]);
        let (task, job) = create(&tasks, &caller, 1, "subscription-job").await;
        let usage = MediaTaskUsageUri::new(task.task_id).unwrap();
        let prediction = MediaPredictionUri::new(job.external_job_id.clone());
        let filter = |uri: &str| {
            rmcp::model::SubscriptionFilter::builder()
                .resource_subscriptions([uri])
                .build()
        };
        assert!(
            subscriptions::authorize(&reader, &caller, &filter(prediction.as_str()))
                .await
                .is_ok()
        );
        let denied = owner(Some("tenant-a"), "other", "operator", &[]);
        assert!(
            subscriptions::authorize(&reader, &denied, &filter(prediction.as_str()))
                .await
                .is_err()
        );
        assert!(
            subscriptions::authorize(&reader, &denied, &filter(usage.as_str()))
                .await
                .is_err()
        );
        assert!(
            subscriptions::authorize(&reader, &caller, &filter("media://usage?unknown=1"))
                .await
                .is_err()
        );
        db.a.client()
            .query("DELETE media_usage WHERE task = $task; DELETE ONLY $job;")
            .bind(("task", task_record_id(task.task_id)))
            .bind(("job", job.job_id.record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
        // Usage can be observed before the first record. Missing predictions cannot.
        assert!(
            subscriptions::authorize(&reader, &caller, &filter(usage.as_str()))
                .await
                .is_ok()
        );
        assert!(
            subscriptions::authorize(&reader, &caller, &filter(prediction.as_str()))
                .await
                .is_err()
        );
        let state = MediaState::new(db.a.clone());
        let estimate = UsageRecord {
            task_id: task.task_id.to_string(),
            provider_job_id: None,
            source_id: Some("before-submission".into()),
            model_id: "test/image".into(),
            kind: UsageKind::Estimate,
            quantity: Some(1.),
            unit: Some("run".into()),
            amount: None,
            currency: None,
            recorded_at: Utc::now(),
            metadata: json!({}),
        };
        state.record_usage(&task, None, &estimate).await.unwrap();
        assert_eq!(reads.usage(&caller, &usage).await.unwrap().len(), 1);
        assert_eq!(
            reads.usage_page(&caller, None).await.unwrap().items().len(),
            1
        );
        let mut wrong = estimate.clone();
        wrong.provider_job_id = Some(job.external_job_id.to_string());
        assert!(state.record_usage(&task, None, &wrong).await.is_err());
        wrong = estimate;
        wrong.task_id = TaskId::new().to_string();
        assert!(state.record_usage(&task, None, &wrong).await.is_err());
        db.a.client()
            .query("UPDATE ONLY $task SET request.owner.profile = 'inconsistent' RETURN NONE;")
            .bind(("task", task_record_id(task.task_id)))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(reads.usage(&caller, &usage).await.unwrap().is_empty());
        assert!(
            reads
                .usage_page(&caller, None)
                .await
                .unwrap()
                .items()
                .is_empty()
        );
        assert!(
            subscriptions::authorize(&reader, &caller, &filter(usage.as_str()))
                .await
                .is_err()
        );
        for root in [MediaUsageIndexUri::ROOT, MediaPredictionIndexUri::ROOT] {
            assert!(
                subscriptions::authorize(&reader, &caller, &filter(root))
                    .await
                    .is_ok()
            );
        }
    })
    .await
    .expect("Media subscription selection exceeded 60 seconds");
}

async fn store_result(tasks: &TaskRuntime, task: TaskId, result: serde_json::Value) {
    let result = veoveo_platform_store::TaskResultRecord::new(result);
    tasks
        .platform_store()
        .client()
        .query("UPDATE ONLY $task SET result = $result RETURN NONE;")
        .bind(("task", task_record_id(task)))
        .bind(("result", result))
        .await
        .unwrap()
        .check()
        .unwrap();
}

#[tokio::test]
async fn current_generation_results_survive_cross_replica_reads_and_reconnects() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = store::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "media", "writer");
        let reader = TaskRuntime::new(db.b.clone(), "media", "reader");
        let reads = MediaReads::new(&reader).unwrap();
        let caller = owner(Some("tenant-a"), "owner", "operator", &[]);
        let (task, job) = create(&writer, &caller, 1, "provider/id?with-reserved").await;
        let expected = generation_fixture::generation(task.task_id, job.external_job_id);
        let stored =
            serde_json::to_value(task_results::generation_tool_result(expected.clone()).unwrap())
                .unwrap();
        assert!(
            reads
                .generation_result(&caller, expected.result_uri())
                .await
                .unwrap()
                .is_none()
        );
        let working = task_results::get_task(
            &reader,
            &caller,
            rmcp::model::GetTaskParams::new(task.task_id.to_string()),
        )
        .await
        .unwrap();
        assert!(matches!(
            working.task.payload,
            rmcp::model::TaskPayload::Working
        ));
        writer
            .claim(&task.task_id.to_string(), Duration::from_secs(30))
            .await
            .unwrap();
        writer
            .transition(
                &task.task_id.to_string(),
                veoveo_task_runtime::TaskTransition::Succeeded {
                    message: task_results::GENERATION_COMPLETED.into(),
                    result: stored.clone(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            reads
                .generation_result(&caller, expected.result_uri())
                .await
                .unwrap(),
            Some(expected.clone())
        );
        let delivered = task_results::get_task(
            &reader,
            &caller,
            rmcp::model::GetTaskParams::new(task.task_id.to_string()),
        )
        .await
        .unwrap();
        assert_eq!(
            delivered.task.task.status_message.as_deref(),
            Some(task_results::GENERATION_COMPLETED)
        );
        let rmcp::model::TaskPayload::Completed { result } = &delivered.task.payload else {
            panic!("completed Media Task expected");
        };
        assert_eq!(*result, stored.as_object().unwrap().clone());
        for _ in 0..2 {
            let mut subscription = task_results::subscribe_tasks(
                &reader,
                caller.clone(),
                vec![task.task_id.to_string()],
            )
            .await
            .unwrap();
            assert_eq!(subscription.accepted_task_ids, [task.task_id.to_string()]);
            assert_eq!(
                subscription.updates.next().await.unwrap().unwrap(),
                delivered.task
            );
        }
        assert_eq!(
            reader
                .get(&task.task_id.to_string())
                .await
                .unwrap()
                .unwrap()
                .result,
            Some(stored)
        );
        // The immutable result is an exact read; no subscription is advertised.
        let filter = rmcp::model::SubscriptionFilter::builder()
            .resource_subscriptions([expected.result_uri().as_str()])
            .build();
        assert!(
            subscriptions::authorize(&reader, &caller, &filter)
                .await
                .is_err()
        );
    })
    .await
    .expect("Media current result qualification exceeded 60 seconds");
}

#[test]
fn generation_handoff_has_one_canonical_link_and_identity_free_status() {
    let generation = generation_fixture::generation(
        TaskId::new(),
        MediaPredictionId::new("provider-job").unwrap(),
    );
    let result = task_results::generation_tool_result(generation.clone()).unwrap();
    assert_eq!(result.content.len(), 2);
    let rmcp::model::ContentBlock::Text(text) = &result.content[0] else {
        panic!("status text expected");
    };
    assert_eq!(text.text, "Generation completed.");
    let rmcp::model::ContentBlock::ResourceLink(link) = &result.content[1] else {
        panic!("single result link expected");
    };
    assert_eq!(link.uri, generation.result_uri().as_str());
    assert_eq!(link.mime_type.as_deref(), Some("application/json"));
    assert_eq!(
        result.structured_content,
        Some(serde_json::to_value(&generation).unwrap())
    );
    assert_eq!(result.is_error, Some(false));
}

#[tokio::test]
async fn generation_selection_excludes_denied_malformed_results_before_decoding() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = store::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "media", "writer");
        let reader = TaskRuntime::new(db.b.clone(), "media", "reader");
        let reads = MediaReads::new(&reader).unwrap();
        let caller = owner(Some("tenant-a"), "owner", "operator", &["mission"]);
        let denied = [
            owner(Some("tenant-b"), "owner", "operator", &[]),
            owner(Some("tenant-a"), "other", "operator", &[]),
            owner(Some("tenant-a"), "owner", "observer", &[]),
            owner(Some("tenant-a"), "owner", "operator", &["secret"]),
        ];
        for (index, denied) in denied.iter().enumerate() {
            let prediction = if index == 0 { "collision".to_owned() } else { format!("denied-{index}") };
            let (task, job) = create(&writer, denied, index as u64 + 1, &prediction).await;
            store_result(&writer, task.task_id, json!({"structuredContent": {
                "schema":"unsupported", "prediction":{"id":prediction}
            }})).await;
            db.a.client().query("UPDATE ONLY $task SET status = 'succeeded' RETURN NONE;")
                .bind(("task", task_record_id(task.task_id))).await.unwrap().check().unwrap();
            assert!(reads.generation_result(&caller, &MediaGenerationUri::new(job.external_job_id)).await.unwrap().is_none());
            assert!(reads.generation_for_task(&caller, task.task_id).await.unwrap().is_none());
        }
        let (task, job) = create(&writer, &caller, 10, "collision").await;
        let expected = generation_fixture::generation(task.task_id, job.external_job_id);
        store_result(&writer, task.task_id, json!({"structuredContent": expected})).await;
        db.a.client().query("UPDATE ONLY $task SET status = 'succeeded' RETURN NONE;")
            .bind(("task", task_record_id(task.task_id))).await.unwrap().check().unwrap();
        assert_eq!(reads.generation_result(&caller, expected.result_uri()).await.unwrap(), Some(expected.clone()));
        db.a.client().query("UPDATE ONLY $task SET request.owner.data_labels = ['mission','secret'] RETURN NONE;")
            .bind(("task", task_record_id(task.task_id))).await.unwrap().check().unwrap();
        assert!(reads.generation_result(&caller, expected.result_uri()).await.unwrap().is_none());
        assert!(reads.generation_for_task(&caller, task.task_id).await.unwrap().is_none());
        let mut cleared = caller.clone();
        cleared.data_labels.insert("secret".into());
        assert_eq!(reads.generation_result(&cleared, expected.result_uri()).await.unwrap(), Some(expected));
        let implicit = owner(None, "owner", "operator", &[]);
        let explicit = owner(Some("installation"), "owner", "operator", &[]);
        for (index, (allowed, denied)) in [(&implicit, &explicit), (&explicit, &implicit)].into_iter().enumerate() {
            let (task, job) = create(&writer, allowed, index as u64 + 20, &format!("optional-tenant-{index}")).await;
            let expected = generation_fixture::generation(task.task_id, job.external_job_id);
            store_result(&writer, task.task_id, json!({"structuredContent": expected})).await;
            db.a.client().query("UPDATE ONLY $task SET status = 'succeeded' RETURN NONE;")
                .bind(("task", task_record_id(task.task_id))).await.unwrap().check().unwrap();
            assert!(reads.generation_result(denied, expected.result_uri()).await.unwrap().is_none());
            assert!(reads.generation_for_task(denied, task.task_id).await.unwrap().is_none());
            assert_eq!(reads.generation_result(allowed, expected.result_uri()).await.unwrap(), Some(expected));
        }
    }).await.expect("Media result visibility qualification exceeded 60 seconds");
}

#[tokio::test]
async fn generation_results_require_success_and_consistent_retained_parents() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = store::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "media", "writer");
        let reader = TaskRuntime::new(db.b.clone(), "media", "reader");
        let reads = MediaReads::new(&reader).unwrap();
        let caller = owner(Some("tenant-a"), "owner", "operator", &[]);
        for (index, mutation) in [
            "UPDATE ONLY $task SET status = 'failed' RETURN NONE;",
            "UPDATE ONLY $task SET status = 'cancelled' RETURN NONE;",
            "UPDATE ONLY $task SET result.payload.isError = true RETURN NONE;",
            "UPDATE ONLY $task SET result.payload.structuredContent.prediction.id = 'wrong' RETURN NONE;",
            "UPDATE ONLY $task SET request.owner.profile = 'wrong' RETURN NONE;",
            "UPDATE ONLY $task SET tenant = tenant:missing RETURN NONE;",
            "UPDATE ONLY $task SET server = mcp_server:other RETURN NONE;",
            "UPDATE ONLY $job SET provider_payload.id = 'wrong' RETURN NONE;",
            "UPDATE ONLY $job SET tenant = tenant:missing RETURN NONE;",
            "UPDATE ONLY $job SET provider = 'other' RETURN NONE;",
            "DELETE ONLY $task;",
            "DELETE ONLY $job;",
        ].into_iter().enumerate() {
            let (task, job) = create(&writer, &caller, index as u64 + 1, &format!("parent-{index}")).await;
            let expected = generation_fixture::generation(task.task_id, job.external_job_id);
            store_result(&writer, task.task_id, json!({"structuredContent": expected})).await;
            db.a.client().query("UPDATE ONLY $task SET status = 'succeeded' RETURN NONE;")
                .bind(("task", task_record_id(task.task_id))).await.unwrap().check().unwrap();
            assert!(reads.generation_result(&caller, expected.result_uri()).await.unwrap().is_some());
            db.a.client().query(mutation).bind(("task", task_record_id(task.task_id)))
                .bind(("job", job.job_id.record_id())).await.unwrap().check().unwrap();
            assert!(reads.generation_result(&caller, expected.result_uri()).await.unwrap().is_none(), "accepted {mutation}");
            assert!(reads.generation_for_task(&caller, task.task_id).await.unwrap().is_none(), "Task selection accepted {mutation}");
        }
        let (task, job) = create(&writer, &caller, 100, "invalid-visible").await;
        let expected = generation_fixture::generation(task.task_id, job.external_job_id.clone());
        db.a.client().query("UPDATE ONLY $task SET status = 'succeeded' RETURN NONE;")
            .bind(("task", task_record_id(task.task_id))).await.unwrap().check().unwrap();
        let wrong_parent = generation_fixture::generation(TaskId::new(), job.external_job_id);
        store_result(&writer, task.task_id, json!({"structuredContent": wrong_parent})).await;
        assert!(reads.generation_result(&caller, expected.result_uri()).await.is_err());
        assert!(reads.generation_for_task(&caller, task.task_id).await.is_err());
        for profile in [
            json!({"prediction": expected.prediction(), "artifacts": expected.artifacts()}),
            json!({"schema":"veoveo.ai/media-generation/v2", "prediction":expected.prediction(), "artifacts": expected.artifacts()}),
            json!({"prediction": expected.prediction(), "artifacts": [], "unknown": true}),
        ] {
            store_result(&writer, task.task_id, json!({"structuredContent": profile})).await;
            assert!(reads.generation_result(&caller, expected.result_uri()).await.is_err());
        }
    }).await.expect("Media result integrity qualification exceeded 60 seconds");
}
