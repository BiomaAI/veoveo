//! Task settlement only: inert source/output metadata, no inference or Artifact writes.
use super::*;
use crate::{store_fixture as fixture, test_support as support};
use std::collections::BTreeSet;
use veoveo_artifact_contract::*;
use veoveo_speech_contract::{
    SpeechTaskKind, TranscribeRequest, TranscriptionOutputValue, TranscriptionUri,
};
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskError, TaskRuntime};
use veoveo_types::{TaskId, TaskTypeDefinition};

fn metadata(mime: &str) -> ArtifactMetadata {
    ArtifactMetadata {
        artifact_uri: ArtifactId::new().plane_uri(),
        byte_len: 1,
        mime_type: Some(mime.into()),
        filename: None,
        download_url: None,
        created_at: chrono::Utc::now(),
        release_state: Default::default(),
        compliance: Default::default(),
        metadata: serde_json::json!({}),
    }
}

async fn create(runtime: &TaskRuntime) -> TaskSnapshot {
    let id = TaskId::new();
    let source = metadata("audio/wav");
    let artifact_task = ArtifactTaskId::parse(id.to_string()).unwrap();
    let expires_at = chrono::Utc::now() + chrono::TimeDelta::hours(1);
    let request = DurableRequest {
        input: TranscribeRequest {
            artifact_uri: source.artifact_uri.clone(),
        },
        source,
        read: IssuedArtifactReadCapability {
            capability_id: ArtifactReadCapabilityId::new(),
            task_id: artifact_task,
            expires_at,
            secret: ArtifactReadCapabilitySecret::new("inert_fixture_capability_not_issued_0000")
                .unwrap(),
        },
        write: IssuedArtifactWriteCapability {
            capability_id: ArtifactWriteCapabilityId::new(),
            task_id: artifact_task,
            expires_at,
            secret: ArtifactWriteCapabilitySecret::new("inert_fixture_capability_not_issued_0000")
                .unwrap(),
        },
    };
    runtime
        .create(CreateTask {
            task_id: id,
            owner: support::owner("alice"),
            server: "speech".into(),
            task_type: SpeechTaskKind::Transcribe.name(),
            request: serde_json::to_value(request).unwrap(),
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: None,
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap();
    runtime.claim(id, LEASE).await.unwrap().snapshot
}

fn completion(before: &TaskSnapshot) -> TaskTransition {
    let request: DurableRequest = serde_json::from_value(before.request.clone()).unwrap();
    let provenance = serde_json::json!({"sourceArtifactUri": request.source.artifact_uri,
        "sourceSha256": "a".repeat(64), "model":"inert-fixture", "modelRevision":"fixture"});
    let mut transcript = metadata("application/json")
        .presented_under_scheme(&veoveo_speech_contract::ARTIFACT_SCHEME);
    transcript.metadata = provenance.clone();
    let mut captions =
        metadata("text/vtt").presented_under_scheme(&veoveo_speech_contract::ARTIFACT_SCHEME);
    captions.metadata = provenance;
    let output = TranscriptionOutputValue {
        result_uri: TranscriptionUri::new(TranscriptionId::try_from(before.task_id).unwrap()),
        source_artifact_uri: request.source.artifact_uri,
        transcript,
        captions,
        duration_seconds: 1.0,
    }
    .build()
    .unwrap();
    let mut result =
        rmcp::model::CallToolResult::success(vec![rmcp::model::ContentBlock::ResourceLink(
            rmcp::model::Resource::new(output.result_uri.to_string(), "Transcript")
                .with_mime_type("application/json"),
        )]);
    result.structured_content = Some(serde_json::to_value(output).unwrap());
    veoveo_task_runtime::mcp_task_completion("Transcript ready", result).unwrap()
}

#[tokio::test]
async fn resumable_transcript_settlement_cancels_failures_and_preserves_completed_links() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "speech", "speech-writer");
        let reader = TaskRuntime::new(db.b.clone(), "speech", "speech-canceller");
        for kind in 0..3 {
            let before = create(&writer).await;
            let transition = if kind == 1 {
                TaskTransition::Failed(TaskFailure::new("transcription_failed", "known failure"))
            } else {
                completion(&before)
            };
            if kind < 2 {
                reader.cancel(before.task_id).await.unwrap();
                assert!(matches!(
                    writer
                        .transition_if_current(&before, transition.clone())
                        .await,
                    Err(TaskError::Conflict(_))
                ));
            }
            let after = writer
                .transition_resumable_if_current(&before, transition, CANCELLATION_POLICY, None)
                .await
                .unwrap();
            assert_eq!(after.owner, before.owner);
            assert_eq!(after.request, before.request);
            if kind < 2 {
                assert_eq!(after.status, TaskStatus::Cancelled);
                assert!(after.result.is_none() && after.result_uri.is_none());
            } else {
                assert_eq!(after.status, TaskStatus::Succeeded);
                assert_eq!(reader.cancel(before.task_id).await.unwrap(), after);
                let output = SpeechService::output(&after).unwrap().unwrap();
                assert_eq!(
                    after.result_uri.as_ref().unwrap(),
                    &output.result_uri.to_uri()
                );
            }
        }
        let before = create(&writer).await;
        let stop = CancellationToken::new();
        stop.cancel();
        assert_eq!(
            writer
                .transition_resumable_if_current(
                    &before,
                    completion(&before),
                    CANCELLATION_POLICY,
                    Some(&stop)
                )
                .await
                .unwrap(),
            before
        );
        assert!(
            reader
                .get(before.task_id)
                .await
                .unwrap()
                .unwrap()
                .result
                .is_none()
        );
    })
    .await
    .expect("Speech resumable settlement controls exceeded 60 seconds");
}

#[tokio::test]
async fn cancelled_progress_stops_before_the_next_effect() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "speech", "speech-progress-writer");
        let canceller = TaskRuntime::new(db.b.clone(), "speech", "speech-progress-canceller");
        let before = create(&writer).await;
        canceller.cancel(before.task_id).await.unwrap();
        let effects = std::sync::atomic::AtomicUsize::new(0);
        let work = async {
            progress_checkpoint(
                &writer,
                TranscriptionId::try_from(before.task_id)?,
                "Reading authorized recording",
                0.0,
            )
            .await?;
            effects.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok::<_, anyhow::Error>(())
        };
        assert!(work.await.is_err());
        assert_eq!(effects.load(std::sync::atomic::Ordering::SeqCst), 0);
        let current = writer.get(before.task_id).await.unwrap().unwrap();
        assert_eq!(current.status, TaskStatus::Cancelled);
        assert!(current.result.is_none() && current.result_uri.is_none());
        // A terminal completion also forbids starting another effect at progress.
        let before = create(&writer).await;
        writer
            .transition(before.task_id, completion(&before))
            .await
            .unwrap();
        assert!(
            progress_checkpoint(
                &writer,
                TranscriptionId::try_from(before.task_id).unwrap(),
                "Publishing transcript",
                0.99
            )
            .await
            .is_err()
        );
    })
    .await
    .expect("Speech progress checkpoint control exceeded 60 seconds");
}
