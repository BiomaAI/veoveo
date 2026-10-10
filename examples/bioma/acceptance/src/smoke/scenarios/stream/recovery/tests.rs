use super::*;
use rmcp::model::TaskPayload;
use serde_json::json;

fn fixture() -> Result<(
    veoveo_deploy_contract::InstallationTarget,
    Fixture,
    AnalysisResults,
)> {
    let mut target = veoveo_deploy_contract::InstallationTarget::decode(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../testing/fixtures/catalog-installation/installation-target.json"
    )))?;
    target.expected_deployments.push("stream-mcp".into());
    let results: AnalysisResults = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../servers/stream-mcp/testdata/replay-results-v2.json"
    )))?;
    let video = RecordingVideoSelection::new(
        results.recording_uri.clone(),
        results.entity_path.clone(),
        results.timeline.clone(),
        results.requested_range,
    )?;
    let fixture = Fixture {
        target: CrashTarget {
            deployment: "stream-mcp".into(),
            pod: "stream-selected".into(),
            container: "stream-mcp".into(),
            namespace_uid: uuid::Uuid::now_v7(),
            deployment_uid: uuid::Uuid::now_v7(),
            replica_set_uid: uuid::Uuid::now_v7(),
            pod_uid: uuid::Uuid::now_v7(),
            container_id: "containerd://selected".into(),
            image_id: format!("registry.test/stream@sha256:{}", "a".repeat(64)),
            restart_count: 0,
        },
        replacement_timeout_seconds: 300,
        request: RunRecordingRequest {
            video: video.clone(),
            pipeline_id: results.pipeline_id.clone(),
            sampling: Default::default(),
            include_source_clip: true,
        },
        expected: Expected {
            video,
            pipeline_id: results.pipeline_id.clone(),
            model_id: results.model_id.clone(),
            source_snapshot: results.source_snapshot.clone(),
            processed_frames: results.processed_frames,
            minimum_detections: 4,
        },
    };
    Ok((target, fixture, results))
}
#[test]
fn stream_recovery_admits_only_selected_role_and_independent_request() -> Result<()> {
    let (target, valid, _) = fixture()?;
    valid.admit_target(&target)?;
    let mut bad = valid.clone();
    bad.target.container = "worker-sidecar".into();
    ensure!(bad.admit_target(&target).is_err());
    let mut bad = valid.clone();
    bad.target.deployment = "foreign".into();
    ensure!(bad.admit_target(&target).is_err());
    let mut bad = valid.clone();
    bad.replacement_timeout_seconds = 301;
    ensure!(bad.admit_target(&target).is_err());
    let mut bad = valid.clone();
    bad.expected.pipeline_id = PipelineId::parse("foreign")?;
    ensure!(bad.admit_target(&target).is_err());
    let mut bad = valid.clone();
    bad.request.include_source_clip = false;
    ensure!(bad.admit_target(&target).is_err());
    let mut wire = serde_json::to_value(&valid)?;
    wire["delaySeconds"] = json!(30);
    ensure!(serde_json::from_value::<Fixture>(wire).is_err());
    Ok(())
}
#[test]
fn stream_recovery_refuses_early_terminal_and_replaced_identity() -> Result<()> {
    let id = CanonicalTaskId::parse("opaque-original")?;
    let created = Task::new(
        id.as_str(),
        TaskStatus::Working,
        "2026-10-10T00:00:00Z",
        "2026-10-10T00:00:00Z",
    );
    let current = DetailedTask::new(created.clone(), TaskPayload::Working);
    working(&created, &current, &id)?;
    ensure!(
        working(
            &created,
            &DetailedTask::new(created.clone(), TaskPayload::Cancelled),
            &id
        )
        .is_err()
    );
    let mut foreign = current.clone();
    foreign.task.task_id = "opaque-foreign".into();
    ensure!(working(&created, &foreign, &id).is_err());
    let mut foreign = current;
    foreign.task.created_at = "2026-10-10T00:00:01Z".into();
    ensure!(working(&created, &foreign, &id).is_err());
    Ok(())
}
#[test]
fn stream_recovery_checks_current_product_and_immutable_sources() -> Result<()> {
    let (_, fixture, results) = fixture()?;
    let mut output: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../servers/stream-mcp/testdata/run-output.json"
    )))?;
    output["summary"]["processedFrames"] = json!(results.processed_frames);
    output["summary"]["detectionCount"] = json!(4);
    output["summary"]["requestedStartIndex"] = json!(0);
    output["summary"]["requestedEndIndex"] = json!(30);
    let digest = results.source_snapshot.digest_sha256()?.to_string();
    let digest = digest.strip_prefix("sha256:").unwrap_or(&digest);
    output["resultsArtifact"]["metadata"]["provenance"]["sourceSnapshotSha256"] = json!(digest);
    output["annotationsArtifact"]["metadata"]["provenance"]["sourceSnapshotSha256"] = json!(digest);
    let mut clip = output["resultsArtifact"].clone();
    clip["artifactId"] = json!("01983da0-0000-7000-8000-000000000004");
    clip["artifactUri"] = json!("stream://artifact/01983da0-0000-7000-8000-000000000004");
    clip["metadata"]["provenance"] = json!({"kind":"stream_source_clip", "runId":output["resultsArtifact"]["metadata"]["provenance"]["runId"], "recordingId":results.source_snapshot.recording_id, "entityPath":results.entity_path, "timeline":results.timeline, "decodeStartIndex":0, "sourceSnapshotSha256":digest});
    output["sourceClipArtifact"] = clip;
    let admitted: RunRecordingOutput = serde_json::from_value(output.clone())?;
    fixture.product(&admitted, &results)?;
    let mut wrong = fixture.clone();
    wrong.expected.processed_frames += 1;
    ensure!(wrong.product(&admitted, &results).is_err());
    let mut wrong = fixture.clone();
    wrong.expected.model_id = ModelId::parse("foreign")?;
    ensure!(wrong.product(&admitted, &results).is_err());
    output["summary"]["detectionCount"] = json!(5);
    ensure!(
        wrong
            .product(&serde_json::from_value(output)?, &results)
            .is_err()
    );
    Ok(())
}
#[tokio::test]
async fn stream_recovery_interrupted_close_keeps_original_cap_and_sticky_failure() -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let root = tempfile::tempdir()?;
    let evidence = Evidence {
        journal: Arc::new(
            veoveo_testing_support::final_tasks::public_caller::PrivateCallerJournal::create(
                &root.path().join("outcome.jsonl"),
            )?,
        ),
        facts: SyncMutex::new(Facts::default()),
    };
    let (send, receive) = tokio::sync::oneshot::channel();
    let polls = Arc::new(AtomicUsize::new(0));
    let counted = polls.clone();
    let end = tokio::time::Instant::now() + Duration::from_millis(30);
    let mut handles = Handles {
        total_end: Some(end),
        watch_close: Some(Box::pin(async move {
            counted.fetch_add(1, Ordering::SeqCst);
            receive.await.unwrap_or(false)
        })),
        ..Default::default()
    };
    {
        let close = handles.close_until(&evidence, end);
        tokio::pin!(close);
        std::future::poll_fn(|context| {
            assert!(close.as_mut().poll(context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
    }
    ensure!(polls.load(Ordering::SeqCst) == 1);
    tokio::time::sleep_until(end).await;
    send.send(true)
        .map_err(|_| anyhow::anyhow!("retained close receiver lost"))?;
    ensure!(
        handles
            .close_until(&evidence, end + Duration::from_secs(60))
            .await
            .is_err()
    );
    ensure!(
        handles
            .close_until(&evidence, end + Duration::from_secs(60))
            .await
            .is_err()
    );
    ensure!(handles.end == Some(end) && handles.failed && handles.watch_close.is_some());
    let facts = evidence.facts.lock().unwrap();
    ensure!(facts.cleanup_failed && facts.failed && !facts.watch_closed);
    Ok(())
}

#[tokio::test]
async fn stream_recovery_ready_false_close_is_not_repolled_and_client_drains() -> Result<()> {
    completed_failure_retry(false).await
}
#[tokio::test]
async fn stream_recovery_ready_error_close_is_not_repolled_and_watch_drains() -> Result<()> {
    completed_failure_retry(true).await
}
async fn completed_failure_retry(client_fails: bool) -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let root = tempfile::tempdir()?;
    let evidence = Evidence {
        journal: Arc::new(
            veoveo_testing_support::final_tasks::public_caller::PrivateCallerJournal::create(
                &root.path().join("outcome.jsonl"),
            )?,
        ),
        facts: SyncMutex::new(Facts::default()),
    };
    let watch_drained = Arc::new(AtomicUsize::new(0));
    let client_drained = Arc::new(AtomicUsize::new(0));
    let watch_count = watch_drained.clone();
    let client_count = client_drained.clone();
    let (finish_client, client_ready) = tokio::sync::oneshot::channel();
    let end = tokio::time::Instant::now() + Duration::from_secs(1);
    let mut handles = Handles {
        total_end: Some(end),
        watch_close: Some(Box::pin(async move {
            watch_count.fetch_add(1, Ordering::SeqCst);
            client_fails
        })),
        client_close: Some(Box::pin(async move {
            client_count.fetch_add(1, Ordering::SeqCst);
            client_ready.await?;
            ensure!(!client_fails, "selected client close failure");
            Ok(())
        })),
        ..Default::default()
    };
    {
        let close = handles.close_until(&evidence, end);
        tokio::pin!(close);
        std::future::poll_fn(|context| {
            assert!(close.as_mut().poll(context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
    }
    ensure!(handles.watch_close.is_none() && handles.client_close.is_some());
    ensure!(client_fails || handles.failed);
    finish_client
        .send(())
        .map_err(|_| anyhow::anyhow!("original client close lost"))?;
    ensure!(handles.close_until(&evidence, end).await.is_err());
    ensure!(watch_drained.load(Ordering::SeqCst) == 1);
    ensure!(client_drained.load(Ordering::SeqCst) == 1);
    ensure!(handles.watch_close.is_none() && handles.client_close.is_none());
    // Re-enter the same cleanup as registered owner cleanup would. Both completed
    // async futures would panic if repolled; their sticky failure must survive.
    ensure!(
        handles
            .close_until(&evidence, end + Duration::from_secs(60))
            .await
            .is_err()
    );
    ensure!(watch_drained.load(Ordering::SeqCst) == 1);
    ensure!(client_drained.load(Ordering::SeqCst) == 1);
    ensure!(handles.end == Some(end) && handles.failed);
    ensure!(evidence.facts.lock().unwrap().cleanup_failed);
    Ok(())
}

#[tokio::test]
async fn stream_recovery_final_journal_failure_stays_failed_on_cleanup_retry() -> Result<()> {
    // A serializer panic poisons the actual journal's writer lock. This gives the
    // real final append a deterministic failure without replacing its implementation.
    struct InterruptedWrite;
    impl Serialize for InterruptedWrite {
        fn serialize<S: serde::Serializer>(&self, _: S) -> std::result::Result<S::Ok, S::Error> {
            panic!("selected private journal write interruption");
        }
    }
    let root = tempfile::tempdir()?;
    let journal = Arc::new(
        veoveo_testing_support::final_tasks::public_caller::PrivateCallerJournal::create(
            &root.path().join("outcome.jsonl"),
        )?,
    );
    ensure!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            journal.append(&InterruptedWrite)
        }))
        .is_err()
    );
    let evidence = Evidence {
        journal,
        facts: SyncMutex::new(Facts::default()),
    };
    let end = tokio::time::Instant::now() + Duration::from_secs(1);
    let mut handles = Handles {
        total_end: Some(end),
        watch_close: Some(Box::pin(async { true })),
        client_close: Some(Box::pin(async { Ok(()) })),
        ..Default::default()
    };
    ensure!(handles.close_until(&evidence, end).await.is_err());
    ensure!(handles.watch_close.is_none() && handles.client_close.is_none());
    ensure!(handles.failed && evidence.facts.lock().unwrap().cleanup_failed);
    ensure!(
        handles
            .close_until(&evidence, end + Duration::from_secs(60))
            .await
            .is_err()
    );
    ensure!(handles.end == Some(end) && handles.failed);
    ensure!(evidence.facts.lock().unwrap().cleanup_failed);
    Ok(())
}
