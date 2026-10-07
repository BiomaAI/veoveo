//! Hosted argument admission with an exited process and unavailable worker socket.
//! This fixture cannot establish GPU inference readiness.
#[path = "../../tests/support/context.rs"]
mod context;
#[path = "../../../../testing/fixtures/tool_inputs.rs"]
mod input_fixture;
use super::*;
use serde_json::json;
use veoveo_mcp_contract::hosting::testing::{self, TestGateway};
#[path = "../../../../testing/fixtures/store.rs"]
mod fixture;

#[tokio::test]
async fn unknown_tool_arguments_complete_before_transcription_admission() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = fixture::TestDb::new().await;
        let worker = Arc::new(WorkerProcess::unavailable().await.unwrap());
        assert!(worker.ready().await.is_err());
        let service = Arc::new(SpeechService::new(
            TaskRuntime::new(db.a.clone(), "speech", "strict-input"),
            HttpArtifactPlane::new("http://127.0.0.1:9"),
            worker,
            1,
            1,
        ));
        let handler = service.clone();
        let gateway = TestGateway::new(
            testing::for_domain::<mcp::SpeechMcp>()
                .handler(move || Hosted::new(mcp::SpeechMcp::new(handler.clone())))
                .build(),
        );
        let discover = gateway.rpc("server/discover", json!({})).await;
        assert!(discover.get("error").is_none(), "{discover}");
        let mut arguments =
            json!({"artifactUri":"artifact://01983da0-0000-7000-8000-000000000000"});
        let _: veoveo_speech_contract::TranscribeRequest =
            serde_json::from_value(arguments.clone()).unwrap();
        arguments["undeclared"] = true.into();
        let body = gateway
            .rpc(
                "tools/call",
                json!({"name":"transcribe","arguments":arguments}),
            )
            .await;
        assert!(body.get("error").is_none(), "{body}");
        assert_eq!(body["result"]["resultType"], "complete", "{body}");
        let result: rmcp::model::CallToolResult =
            serde_json::from_value(body["result"].clone()).unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(
            serde_json::to_string(&result.content)
                .unwrap()
                .contains("undeclared")
        );
        for (tool, arguments) in [
            ("transcribe", json!({"artifact_uri":"artifact://01983da0-0000-7000-8000-000000000000"})),
            ("transcribe", json!({"artifactUri":"artifact://01983da0-0000-7000-8000-000000000000", "artifact_uri":"artifact://01983da0-0000-7000-8000-000000000000"})),
            ("start_dictation", json!({"id":"01983da0-0000-7000-8000-000000000000", "sample_rate":16000})),
            ("start_dictation", json!({"id":"01983da0-0000-7000-8000-000000000000", "sampleRate":16000, "sample_rate":16000})),
        ] {
            let body = gateway.rpc("tools/call", json!({"name":tool,"arguments":arguments})).await;
            assert!(body.get("error").is_none(), "{body}");
            let result: rmcp::model::CallToolResult = serde_json::from_value(body["result"].clone()).unwrap();
            assert_eq!(result.is_error, Some(true), "{body}");
        }
        let cases = input_fixture::ToolInputCase::load(include_bytes!(
            "../../testdata/controlled-inputs.json"
        ));
        assert_eq!(cases.len(), 2);
        for case in cases {
            match case.tool.as_str() {
                "transcribe" => {
                    let _: crate::contract::TranscribeRequest = case.decode();
                }
                "start_dictation" => {
                    let _: crate::contract::dictation::StartDictation = case.decode();
                }
                _ => panic!("unexpected fixture tool"),
            }
            for (location, arguments) in case
                .unknown_fields()
                .into_iter()
                .chain(case.invalid_values())
            {
                let body = gateway
                    .rpc(
                        "tools/call",
                        json!({"name":case.tool,"arguments":arguments}),
                    )
                    .await;
                assert!(
                    body.get("error").is_none(),
                    "{} {location}: {body}",
                    case.branch
                );
                assert_eq!(
                    body["result"]["resultType"], "complete",
                    "{} {location}: {body}",
                    case.branch
                );
                let result: rmcp::model::CallToolResult =
                    serde_json::from_value(body["result"].clone()).unwrap();
                assert_eq!(
                    result.is_error,
                    Some(true),
                    "{} {location}: {body}",
                    case.branch
                );
                case.assert_error(&location, &serde_json::to_string(&result.content).unwrap());
            }
        }
        assert!(service.tasks.list().await.unwrap().is_empty());
        service
            .audit
            .shutdown(Duration::from_secs(5))
            .await
            .unwrap();
    })
    .await
    .expect("Speech argument admission exceeded 120 seconds");
}

#[path = "../../tests/support/signing.rs"]
mod signing;
#[path = "../../tests/support/mod.rs"]
mod support;

struct ObservedListener {
    inner: super::mcp::SpeechListener,
    active: Arc<std::sync::atomic::AtomicUsize>,
    ended: Arc<tokio::sync::Notify>,
}
impl veoveo_task_runtime::DurableListener<super::tasks::SpeechTasks> for ObservedListener {
    fn accepted_subscription_filter(
        &self,
        requested: &rmcp::model::SubscriptionFilter,
    ) -> Option<rmcp::model::SubscriptionFilter> {
        veoveo_task_runtime::DurableListener::accepted_subscription_filter(&self.inner, requested)
    }
    async fn listen(
        &self,
        service: &super::tasks::SpeechTasks,
        context: rmcp::service::SubscriptionContext,
    ) -> Result<(), rmcp::ErrorData> {
        self.active
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        struct Active(
            Arc<std::sync::atomic::AtomicUsize>,
            Arc<tokio::sync::Notify>,
        );
        impl Drop for Active {
            fn drop(&mut self) {
                self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                self.1.notify_one();
            }
        }
        let _active = Active(self.active.clone(), self.ended.clone());
        veoveo_task_runtime::DurableListener::listen(&self.inner, service, context).await
    }
}

/// Native Task/resource watches and Artifact authorization with inference blocked.
/// This exercises no CUDA process or transcription quality.
#[tokio::test]
async fn shared_transcript_watch_preserves_policy_reconnect_and_cancellation() {
    use futures::StreamExt;
    use veoveo_artifact_contract::PutArtifactRequest;
    use veoveo_artifact_service::{
        ArtifactService, ObjectStoreConfig, PlaneAuthenticator, SurrealArtifactRepository,
    };
    use veoveo_mcp_contract::{
        ArtifactPlane, GATEWAY_INTERNAL_TOKEN_ISSUER, PlaneCaller, ServerSlug, TokenIssuer,
    };
    use veoveo_speech_contract::{TranscribeRequest, TranscriptionId, TranscriptionUri};
    use veoveo_task_runtime::{DurableTaskService, TaskResourceSubscriptions};
    use veoveo_types::DataLabelId;
    tokio::time::timeout(Duration::from_secs(120), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = fixture::TestDb::new().await;
        context::register_context(&db.a).await.unwrap();
        let signing = signing::Signing::new();
        let mut identity = support::identity(&support::owner("alice"));
        identity.actor.data_labels.insert(DataLabelId::parse("speech-private").unwrap());
        identity.request_context.as_mut().unwrap().principal = identity.actor.clone();
        let caller = PlaneCaller {
            bearer_token: signing.identity(identity.clone(), "speech", chrono::Utc::now() + chrono::TimeDelta::minutes(5)),
            memberships: identity.actor.group_memberships(), identity,
        };
        let artifacts = ArtifactService::new(SurrealArtifactRepository::new(db.a.clone()), ObjectStoreConfig::Memory.build().unwrap());
        let auth = PlaneAuthenticator::new(TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap(), vec![ServerSlug::parse("speech").unwrap()], signing.trust.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let plane = HttpArtifactPlane::new(format!("http://{}", listener.local_addr().unwrap()));
        let server = tokio::spawn(async move {
            axum::serve(listener, veoveo_artifact_service::http::router(veoveo_artifact_service::http::AppState::new(artifacts, auth))).await.unwrap();
        });
        struct Server(tokio::task::JoinHandle<()>);
        impl Drop for Server { fn drop(&mut self) { self.0.abort(); } }
        let _server = Server(server);
        let source = plane.put(&caller, PutArtifactRequest {
            mime_type: Some("audio/wav".into()), filename: Some("watch-fixture.wav".into()),
            classification: Some(DataLabelId::parse("speech-private").unwrap()), data_labels: Default::default(), retention_expires_at: None, metadata: json!({}),
        }, std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/english.wav")).unwrap()).await.unwrap();
        let worker = Arc::new(WorkerProcess::unavailable().await.unwrap());
        let service = Arc::new(SpeechService::new(TaskRuntime::new(db.a.clone(), "speech", "watch-fixture"), plane.clone(), worker, 0, 1));
        let task = service.transcribe(&caller, TranscribeRequest { artifact_uri: source.artifact_uri.clone() }, Default::default()).await.unwrap();
        let id = TranscriptionId::try_from(task.task_id).unwrap();
        let uri = TranscriptionUri::new(id).to_string();
        let tasks = super::tasks::SpeechTasks(service.clone());
        let listener = super::mcp::SpeechListener { state: service.clone() };
        let filter = |explicit: bool| serde_json::from_value::<rmcp::model::SubscriptionFilter>(json!({
            "taskIds": if explicit { vec![task.task_id.to_string(), task.task_id.to_string()] } else { vec![] },
            "resourceSubscriptions": [uri.clone(), uri.clone()]
        })).unwrap();
        let next = |stream: veoveo_task_runtime::TaskResourceUpdateStream| async move {
            let mut stream = stream;
            let update = tokio::time::timeout(Duration::from_secs(10), stream.next()).await.unwrap().unwrap().unwrap();
            (stream, update)
        };
        for explicit in [false, true] {
            let selected = TaskResourceSubscriptions::from_filter::<TranscriptionUri>(&filter(explicit)).unwrap();
            let (_, update) = next(selected.subscribe(&tasks, &caller).await.unwrap()).await;
            assert_eq!(update.task.is_some(), explicit);
            assert_eq!(update.resources.len(), 1);
            listener.authorize_update(&caller, &update).await.unwrap();
        }
        let oversized: rmcp::model::SubscriptionFilter = serde_json::from_value(json!({"resourceSubscriptions": vec![uri.clone(); 257]})).unwrap();
        assert!(TaskResourceSubscriptions::from_filter::<TranscriptionUri>(&oversized).is_err());
        let immutable: rmcp::model::SubscriptionFilter = serde_json::from_value(json!({"resourceSubscriptions": ["speech://capabilities"]})).unwrap();
        assert!(TaskResourceSubscriptions::from_filter::<TranscriptionUri>(&immutable).is_err());
        let foreign_uri = TranscriptionUri::new(TranscriptionId::new()).to_string();
        let foreign: rmcp::model::SubscriptionFilter = serde_json::from_value(json!({"resourceSubscriptions": [foreign_uri]})).unwrap();
        assert!(TaskResourceSubscriptions::from_filter::<TranscriptionUri>(&foreign).unwrap().subscribe(&tasks, &caller).await.is_err());
        let mut denied = caller.clone();
        denied.identity.actor.data_labels.clear();
        denied.identity.request_context.as_mut().unwrap().principal = denied.identity.actor.clone();
        denied.bearer_token = signing.identity(denied.identity.clone(), "speech", chrono::Utc::now() + chrono::TimeDelta::minutes(5));
        assert!(plane.head(&denied, &source.artifact_id()).await.is_err());
        let before = serde_json::to_value(service.tasks.get(task.task_id).await.unwrap()).unwrap();
        let selection = TaskResourceSubscriptions::from_filter::<TranscriptionUri>(&filter(false)).unwrap();
        assert!(selection.subscribe(&tasks, &denied).await.is_err());
        let (mut stream, update) = next(TaskResourceSubscriptions::from_filter::<TranscriptionUri>(&filter(false)).unwrap().subscribe(&tasks, &caller).await.unwrap()).await;
        assert!(listener.authorize_update(&denied, &update).await.is_err());
        assert_eq!(serde_json::to_value(service.tasks.get(task.task_id).await.unwrap()).unwrap(), before);
        // Reconnecting under admitted authority supplies the current baseline.
        listener.authorize_update(&caller, &update).await.unwrap();
        tasks.cancel_task(&caller, task.task_id.to_string()).await.unwrap();
        loop {
            let update = tokio::time::timeout(Duration::from_secs(10), stream.next()).await.unwrap().unwrap().unwrap();
            listener.authorize_update(&caller, &update).await.unwrap();
            assert!(update.task.is_none());
            assert_eq!(update.resources.len(), 1);
            if service.tasks.get(task.task_id).await.unwrap().unwrap().status == veoveo_task_runtime::TaskStatus::Cancelled { break; }
        }
        drop(stream);
        // A fresh pending operation produces a real post-baseline native update.
        let task = service.transcribe(&caller, TranscribeRequest { artifact_uri: source.artifact_uri.clone() }, Default::default()).await.unwrap();
        let uri = TranscriptionUri::new(TranscriptionId::try_from(task.task_id).unwrap()).to_string();
        let filter = |explicit: bool| serde_json::from_value::<rmcp::model::SubscriptionFilter>(json!({
            "taskIds": if explicit { vec![task.task_id.to_string(), task.task_id.to_string()] } else { vec![] },
            "resourceSubscriptions": [uri.clone(), uri.clone()]
        })).unwrap();
        // The maintained HTTP client establishes the real SubscriptionContext and sink.
        use rmcp::{ClientServiceExt, model::{ClientConfig, ClientCapabilities, Implementation, ProtocolVersion, ServerNotification}, transport::{StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig}};
        use std::sync::atomic::{AtomicUsize, Ordering};
        let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let active = Arc::new(AtomicUsize::new(0)); let ended = Arc::new(tokio::sync::Notify::new());
        let handler = service.clone(); let counters = active.clone(); let endings = ended.clone();
        let hosted = veoveo_mcp_contract::hosting::HostedServer::for_domain::<super::mcp::SpeechMcp>()
            .deployment(&veoveo_mcp_contract::PublicDeployment::new("https://speech.test").unwrap(), true).unwrap()
            .allowed_hosts(vec![address.ip().to_string()]).internal_trust(signing.trust.clone()).unwrap()
            .handler(move || Hosted::new(super::mcp::SpeechMcp::new(handler.clone())).with_tasks(
                veoveo_task_runtime::DurableTasks::with_listener(super::tasks::SpeechTasks(handler.clone()), ObservedListener { inner: super::mcp::SpeechListener { state: handler.clone() }, active: counters.clone(), ended: endings.clone() })
            )).build();
        let server = tokio::spawn(async move { axum::serve(socket, hosted.into_router()).await.unwrap(); });
        let _hosted = Server(server);
        let transport = StreamableHttpClientTransport::with_client(reqwest::Client::builder().no_proxy().connect_timeout(Duration::from_secs(2)).build().unwrap(),
            StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/speech/mcp")).auth_header(caller.bearer_token.clone()));
        let peer = tokio::time::timeout(Duration::from_secs(10), ClientConfig::new(ClientCapabilities::builder().enable_tasks().build(), Implementation::new("speech-watch-fixture", "1"))
            .serve_with_lifecycle(transport, rmcp::ClientLifecycleMode::Discover { preferred_versions: vec![ProtocolVersion::V_2026_07_28] })).await.unwrap().unwrap();
        for explicit in [false, true] {
            let mut subscription = tokio::time::timeout(Duration::from_secs(10), peer.listen(filter(explicit))).await.unwrap().unwrap();
            let mut resources = 0; let mut tasks = 0;
            for _ in 0..(if explicit {2} else {1}) {
                match tokio::time::timeout(Duration::from_secs(10), subscription.next()).await.unwrap().unwrap().unwrap() {
                    ServerNotification::ResourceUpdatedNotification(n) => { assert_eq!(n.params.uri, uri); resources += 1; },
                    ServerNotification::TaskStatusNotification(_) => tasks += 1,
                    other => panic!("unexpected owning watch notification: {other:?}"),
                }
            }
            assert_eq!((resources, tasks), (1, usize::from(explicit)));
            subscription.cancel().await.unwrap();
            tokio::time::timeout(Duration::from_secs(10), async { while active.load(Ordering::SeqCst) != 0 { ended.notified().await; } }).await.unwrap();
            assert!(subscription.next().await.unwrap().is_none());
        }
        let mut dropped = peer.listen(filter(false)).await.unwrap();
        assert!(matches!(dropped.next().await.unwrap().unwrap(), ServerNotification::ResourceUpdatedNotification(_)));
        drop(dropped);
        tokio::time::timeout(Duration::from_secs(10), async { while active.load(Ordering::SeqCst) != 0 { ended.notified().await; } }).await.unwrap();
        // Reconnect, revoke the current source labels, then cause a native Task update.
        let mut revoked = peer.listen(filter(false)).await.unwrap();
        assert!(matches!(revoked.next().await.unwrap().unwrap(), ServerNotification::ResourceUpdatedNotification(_)));
        let artifact = veoveo_platform_store::ArtifactId::from_uuid(source.artifact_id().as_uuid()).record_id();
        db.a.client().query(include_str!("../../tests/queries/resource_watch/labels.surql"))
            .bind(("artifact", artifact.clone())).bind(("labels", vec!["speech-revoked"])).await.unwrap().check().unwrap();
        assert!(plane.head(&caller, &source.artifact_id()).await.is_err());
        let snapshot = service.tasks.get(task.task_id).await.unwrap().unwrap();
        service.tasks.for_owner(&snapshot.owner).cancel(task.task_id).await.unwrap();
        let delivered = tokio::time::timeout(Duration::from_secs(10), revoked.next()).await.unwrap();
        assert!(matches!(delivered, Ok(None) | Err(_)), "revoked source reached notification sink");
        tokio::time::timeout(Duration::from_secs(10), async { while active.load(Ordering::SeqCst) != 0 { ended.notified().await; } }).await.unwrap();
        db.a.client().query(include_str!("../../tests/queries/resource_watch/labels.surql"))
            .bind(("artifact", artifact)).bind(("labels", Vec::<String>::new())).await.unwrap().check().unwrap();
        let mut reconnect = peer.listen(filter(false)).await.unwrap();
        assert!(matches!(reconnect.next().await.unwrap().unwrap(), ServerNotification::ResourceUpdatedNotification(_)));
        reconnect.cancel().await.unwrap();
        peer.cancel().await.unwrap();
        service.audit.shutdown(Duration::from_secs(5)).await.unwrap();
    }).await.expect("Speech shared watch exceeded 120 seconds");
}
