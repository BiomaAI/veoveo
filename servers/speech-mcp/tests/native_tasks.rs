//! Actual CUDA, disposable database and governed Artifact HTTP service.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "support/signing.rs"]
mod signing;
mod support;
use anyhow::{Result, ensure};
use futures::StreamExt;
use std::{path::PathBuf, sync::Arc, time::Duration};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_artifact_service::{
    ArtifactService, ObjectStoreConfig, PlaneAuthenticator, SurrealArtifactRepository,
};
use veoveo_mcp_contract::*;
use veoveo_speech_contract::{TranscribeRequest, TranscriptDocument};
use veoveo_speech_mcp::{
    application::{SpeechService, owner},
    process::WorkerProcess,
};
use veoveo_task_runtime::{TaskRuntime, TaskStatus, subscribe_durable_tasks};

struct Server(tokio::task::JoinHandle<()>);
impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn context(store: &veoveo_platform_store::PlatformStore) -> Result<()> {
    use veoveo_platform_store as db;
    let id = db::deterministic_work_context_id("test", "speech-test")?.record_id();
    let value = db::WorkContextRecord {
        id: id.clone(),
        tenant: db::deterministic_tenant_id("test")?.record_id(),
        context_key: "speech-test".into(),
        title: "Speech native test".into(),
        policy_revision: "test-1".into(),
        output_policy: db::WorkContextOutputPolicyRecord {
            owner_kind: db::ArtifactGrantSubjectKind::Principal,
            owner_key: "https://speech.test#alice".into(),
            initial_grants: vec![],
            classification: None,
            data_labels: vec![],
        },
        memberships: vec![db::WorkContextMembershipRuleRecord {
            level: db::WorkContextMembershipLevel::Contributor,
            principals: vec![],
            groups: vec![],
            roles: vec![],
            oauth_clients: vec!["console".into()],
        }],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let _: Option<db::WorkContextRecord> = store.client().create(id).content(value).await?;
    Ok(())
}

fn caller(signing: &signing::Signing, name: &str) -> PlaneCaller {
    let mut identity = support::identity(&support::owner(name));
    if name == "alice" {
        identity
            .actor
            .data_labels
            .insert(DataLabelId::new("speech-private").unwrap());
        identity.request_context.as_mut().unwrap().principal = identity.actor.clone();
    }
    PlaneCaller {
        bearer_token: signing.identity(
            identity.clone(),
            "speech",
            chrono::Utc::now() + chrono::TimeDelta::minutes(5),
        ),
        memberships: identity.actor.group_memberships(),
        identity,
    }
}

#[tokio::test]
#[ignore = "requires NVIDIA CUDA, the locked runner, pinned model and Docker store fixture"]
async fn governed_transcript_task_observation_and_cancellation() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tokio::time::timeout(Duration::from_secs(100), exercise()).await?
}
async fn exercise() -> Result<()> {
    let db = fixture::TestDb::new().await;
    context(&db.a).await?;
    let signing = signing::Signing::new();
    let alice = caller(&signing, "alice");
    let bob = caller(&signing, "bob");
    let artifacts = ArtifactService::new(
        SurrealArtifactRepository::new(db.a.clone()),
        ObjectStoreConfig::Memory.build()?,
    );
    let auth = PlaneAuthenticator::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        vec![ServerSlug::new("speech")?],
        signing.trust.clone(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let _server = Server(tokio::spawn(async move {
        axum::serve(
            listener,
            veoveo_artifact_service::http::router(veoveo_artifact_service::http::AppState::new(
                artifacts, auth,
            )),
        )
        .await
        .unwrap();
    }));
    let plane = HttpArtifactPlane::new(endpoint);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = plane
        .put(
            &alice,
            PutArtifactRequest {
                mime_type: Some("audio/wav".into()),
                filename: Some("speech-acceptance.wav".into()),
                classification: Some(DataLabelId::new("speech-private")?),
                data_labels: Default::default(),
                retention_expires_at: None,
                metadata: serde_json::json!({}),
            },
            std::fs::read(root.join("testdata/english.wav"))?,
        )
        .await?;
    let worker = Arc::new(WorkerProcess::start(&root.join("runner/.venv/bin/python"), 2).await?);
    let runtime = TaskRuntime::new(db.a.clone(), "speech", "speech-native-worker");
    let reader = TaskRuntime::new(db.b.clone(), "speech", "speech-native-reader");
    let service = Arc::new(SpeechService::new(
        runtime.clone(),
        plane.clone(),
        worker.clone(),
        1,
        1,
    ));
    let input = TranscribeRequest {
        artifact_uri: source.artifact_uri.clone(),
    };
    ensure!(
        service
            .transcribe(&bob, input.clone(), Default::default())
            .await
            .is_err(),
        "unauthorized source accepted"
    );
    let task = service
        .transcribe(&alice, input.clone(), Default::default())
        .await?;
    let id = task.task_id.to_string();
    ensure!(
        service.authorize(&bob, &id, true).await.is_err(),
        "private Task leaked"
    );
    let mut switched = alice.clone();
    switched.identity.authority.work_context = WorkContextId::new("another-context")?;
    ensure!(
        service.authorize(&switched, &id, false).await.is_err(),
        "Task crossed Work Context"
    );
    let mut subscription =
        subscribe_durable_tasks(&reader, owner(&alice.identity), vec![id.clone()])
            .await?
            .updates;
    loop {
        let update = subscription
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("Task subscription closed"))??;
        if matches!(
            update.task.status,
            rmcp::model::TaskStatus::Completed
                | rmcp::model::TaskStatus::Failed
                | rmcp::model::TaskStatus::Cancelled
        ) {
            break;
        }
    }
    let finished = service.authorize(&alice, &id, true).await?;
    ensure!(
        finished.status == TaskStatus::Succeeded,
        "transcription failed: {:?}",
        finished.error
    );
    let output = SpeechService::output(&finished)?
        .ok_or_else(|| anyhow::anyhow!("transcript output missing"))?;
    ensure!(
        output.source_artifact_uri == source.artifact_uri,
        "source identity changed"
    );
    let bytes = plane
        .get(&alice, &output.transcript.artifact_id, AccessLevel::Read)
        .await?;
    let document: TranscriptDocument = serde_json::from_slice(&bytes.bytes)?;
    ensure!(
        document
            .transcript
            .text
            .to_lowercase()
            .contains("old portrait"),
        "GPU transcript missing"
    );
    ensure!(
        output.transcript.compliance.data_labels == source.compliance.data_labels
            && output.transcript.compliance.owner == source.compliance.owner,
        "derived authority changed"
    );
    ensure!(
        plane
            .head(&bob, &output.transcript.artifact_id)
            .await
            .is_err(),
        "transcript Artifact leaked"
    );
    let captions = plane
        .get(&alice, &output.captions.artifact_id, AccessLevel::Read)
        .await?;
    ensure!(
        captions.bytes.starts_with(b"WEBVTT\n"),
        "caption export missing"
    );
    // Fault injection represents a process dying after Artifact publication but
    // before its terminal Task transition. A fresh owner must reuse the output IDs.
    runtime.reap_workers().await;
    let record_id = finished.task_id.record_id();
    let mut record: veoveo_platform_store::TaskRecord =
        db.a.client()
            .select(record_id.clone())
            .await?
            .ok_or_else(|| anyhow::anyhow!("Task record"))?;
    record.status = TaskStatus::Running;
    record.result = None;
    record.result_artifact = None;
    record.completed_at = None;
    record.lease_owner = Some("terminated-worker".into());
    record.lease_expires_at = Some(chrono::Utc::now() - chrono::TimeDelta::seconds(1));
    let _: Option<veoveo_platform_store::TaskRecord> =
        db.a.client().update(record_id).content(record).await?;
    let recovered = reader
        .recover()
        .await?
        .resumable
        .into_iter()
        .find(|task| task.task_id.to_string() == id)
        .ok_or_else(|| anyhow::anyhow!("expired work not recovered"))?;
    let restarted = Arc::new(SpeechService::new(
        reader.clone(),
        plane.clone(),
        worker.clone(),
        1,
        1,
    ));
    restarted.resume(recovered).await?;
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let snapshot = reader.get(&id).await?.unwrap();
            if snapshot.is_terminal() {
                ensure!(
                    snapshot.status == TaskStatus::Succeeded,
                    "recovery failed: {:?}",
                    snapshot.error
                );
                let again = SpeechService::output(&snapshot)?.unwrap();
                ensure!(
                    again.transcript.artifact_id == output.transcript.artifact_id
                        && again.captions.artifact_id == output.captions.artifact_id,
                    "recovery duplicated output"
                );
                break Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    qualify_hosted(restarted, &signing, &alice).await?;
    // Zero recording slots models a queued recording, allowing cancellation to be
    // tested without racing a 0.1-second GPU inference or inventing a slow provider.
    let waiting = Arc::new(SpeechService::new(runtime.clone(), plane, worker, 0, 1));
    let queued = waiting
        .transcribe(&alice, input, Default::default())
        .await?;
    let cancelled = queued.task_id.to_string();
    reader.cancel(&cancelled).await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let snapshot = reader.get(&cancelled).await?.unwrap();
            if snapshot.status == TaskStatus::Cancelled {
                ensure!(snapshot.result.is_none(), "cancelled work published output");
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    runtime.reap_workers().await;
    Ok(())
}

async fn qualify_hosted(
    service: Arc<SpeechService>,
    signing: &signing::Signing,
    caller: &PlaneCaller,
) -> Result<()> {
    use veoveo_mcp_conformance::{
        ConformanceCredentials, HostedServerConformanceProfile, run_hosted_server_conformance,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let base = format!("http://{address}/speech");
    let router = veoveo_speech_mcp::server::router(
        service,
        signing.verifier.clone(),
        vec![address.to_string()],
        "/speech",
        tokio_util::sync::CancellationToken::new(),
    );
    let _server = Server(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    let profile: HostedServerConformanceProfile = serde_json::from_value(serde_json::json!({
        "schemaVersion":"veoveo.io/mcp-conformance-profile/v1", "profileId":"speech-native",
        "contractRevision":"veoveo.io/hosted-mcp/v3", "endpoint":format!("{base}/mcp"),
        "serverSlug":"speech", "ownedResourceSchemes":["speech"],
        "http":{"requireAuthenticationRejection":true,"rejectedHost":"untrusted.invalid", "healthUrl":format!("{base}/healthz"),"readinessUrl":format!("{base}/readyz"),"docsLlmsUrl":format!("{base}/admin/docs/llms.txt")},
        "surfaces":{"tools":"required","resources":"required","resourceTemplates":"required","prompts":"required","completions":"required","tasks":"required","subscriptions":"required","requiredTools":["transcribe","start_dictation","finish_dictation","cancel_dictation"],"requiredResources":["speech://docs","speech://contract"],"requiredResourceTemplates":["speech://transcript/{task_id}","speech://dictation/{id}"],"requiredPrompts":["transcribe_recording"]}
    }))?;
    let report = run_hosted_server_conformance(
        &profile,
        &ConformanceCredentials::bearer(caller.bearer_token.clone()),
    )
    .await?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../output/development/speech");
    std::fs::create_dir_all(&root)?;
    std::fs::write(
        root.join("native-conformance.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    ensure!(
        report.passed(),
        "hosted Speech conformance failed: {:?}",
        report
            .checks
            .iter()
            .filter(|check| check.status != veoveo_mcp_conformance::CheckStatus::Passed)
            .collect::<Vec<_>>()
    );
    Ok(())
}
