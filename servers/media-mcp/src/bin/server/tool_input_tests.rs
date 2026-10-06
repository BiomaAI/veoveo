//! Hosted protocol and callback admission with isolated Store and loopback billing fixtures.
#[path = "../../../../../testing/fixtures/tool_inputs.rs"]
mod input_fixture;
use super::*;
use serde_json::{Value, json};
use std::time::Duration;
use veoveo_mcp_contract::hosting::testing::{self, TestGateway};
#[path = "../../../../../testing/fixtures/store.rs"]
mod fixture;

#[tokio::test]
async fn unknown_tool_arguments_complete_without_provider_dispatch() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = fixture::TestDb::with_modules(vec![
            veoveo_media_mcp::schema::module_setup(
                fixture::module_lanes::execution("media").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let args = config::Args::try_parse_from([
            "server",
            "--public-base-url",
            "https://media.test",
            "--api-key",
            "fixture",
            "--webhook-secret",
            "fixture",
            "--surreal-endpoint",
            "ws://127.0.0.1:9/rpc",
            "--surreal-namespace",
            "fixture",
            "--surreal-database",
            "fixture",
            "--surreal-auth-level",
            "database",
            "--surreal-username",
            "fixture",
            "--surreal-password",
            "fixture",
            "--internal-trust-jwks",
            r#"{"keys":[]}"#,
        ])
        .unwrap();
        let state = Arc::new(AppState {
            provider: veoveo_media_mcp::provider::ProviderClient::new("fixture")
                .with_base("http://127.0.0.1:9")
                .unwrap(),
            http: reqwest::Client::new(),
            public_endpoint: veoveo_mcp_contract::PublicDeployment::new("https://media.test")
                .unwrap()
                .server("media")
                .unwrap(),
            webhook_secret: secrecy::SecretString::from("fixture"),
            registry: tokio::sync::RwLock::new(None),
            registry_install_attempts: Default::default(),
            tasks: TaskRuntime::new(db.a.clone(), "media", "strict-input"),
            durable: veoveo_media_mcp::state::MediaState::new(db.a.clone()),
            artifacts: ArtifactRepository::new("http://127.0.0.1:9"),
            retention: args.retention_policy(),
            subscribers: SubscriptionHub::new(),
        });
        let models: Vec<_> = (0..237)
            .map(|n| veoveo_media_mcp::contract::ModelEntry {
                model_id: format!("fixture/model-{n:03}").parse().unwrap(),
                name: format!("Model {n}"),
                model_type: "image".into(),
                description: "Provider description".into(),
                base_price: Some(0.1),
                formula: None,
                api_schema: Some(json!({"providerExtension": {"open": true}})),
            })
            .collect();
        *state.registry.write().await =
            Some(app_state::RegistryCache::admit(models.clone()).unwrap());
        // Refreshes retain each admitted vector/index as one immutable snapshot.
        let requested = models[236].model_id.clone();
        let old = state.registry_snapshot().await.unwrap();
        let mut reordered = models.clone();
        reordered.reverse();
        *state.registry.write().await = Some(app_state::RegistryCache::admit(reordered).unwrap());
        assert_eq!(
            state
                .find_model(&requested)
                .await
                .unwrap()
                .unwrap()
                .model_id,
            requested
        );
        let short = vec![models[236].clone()];
        *state.registry.write().await = Some(app_state::RegistryCache::admit(short).unwrap());
        // A retained earlier snapshot remains usable while actual lookup sees the short refresh.
        assert_eq!(old.models_for_test().len(), 237);
        assert_eq!(old.find_model(&requested).unwrap().model_id, requested);
        assert_eq!(
            state
                .find_model(&requested)
                .await
                .unwrap()
                .unwrap()
                .model_id,
            requested
        );
        assert!(
            state
                .find_model(&models[0].model_id)
                .await
                .unwrap()
                .is_none()
        );
        *state.registry.write().await = Some(app_state::RegistryCache::admit(models).unwrap());
        let handler = state.clone();
        let gateway = TestGateway::new(
            testing::for_domain::<MediaMcp>()
                .handler(move || Hosted::new(MediaMcp::new(handler.clone())))
                .build(),
        );
        let discover = gateway.rpc("server/discover", json!({})).await;
        assert!(discover.get("error").is_none(), "{discover}");
        // Traverse actual hosted tool and resource interfaces without provider dispatch.
        let mut cursor = None;
        let mut seen = std::collections::BTreeSet::new();
        loop {
            let args = veoveo_media_mcp::contract::ModelsArgs {
                query: None,
                model_type: None,
                limit: Some(100),
                cursor: cursor.clone(),
            };
            let tool = gateway
                .rpc("tools/call", json!({"name": "models", "arguments": args}))
                .await;
            assert!(tool.get("error").is_none(), "{tool}");
            assert_ne!(tool["result"]["isError"], true);
            let page: veoveo_media_mcp::contract::ModelCatalogOutput =
                serde_json::from_value(tool["result"]["structuredContent"].clone()).unwrap();
            let uri = veoveo_media_mcp::contract::MediaModelIndexUri::new(
                None,
                None,
                args.limit.as_ref(),
                args.cursor.as_ref(),
            );
            let resource = gateway
                .rpc("resources/read", json!({"uri": uri.as_str()}))
                .await;
            assert!(resource.get("error").is_none(), "{resource}");
            let resource_page: veoveo_media_mcp::contract::ModelCatalogOutput =
                serde_json::from_str(resource["result"]["contents"][0]["text"].as_str().unwrap())
                    .unwrap();
            assert_eq!(resource_page, page);
            for model in &page.models {
                assert!(seen.insert(model.model_id.clone()));
            }
            cursor = page.next_cursor.clone();
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(seen.len(), 237);
        let registry_before = state.registry_snapshot().await.unwrap();
        let installs_before = state
            .registry_install_attempts
            .load(std::sync::atomic::Ordering::SeqCst);
        let mut response =
            db.a.client()
                .query(include_str!("queries/invalid_input_state.surql"))
                .await
                .unwrap()
                .check()
                .unwrap();
        let storage_before: surrealdb::types::Value = response.take(0).unwrap();
        let mut arguments = json!({"model":"fixture/image","input":{}});
        let _: RunArgs = serde_json::from_value(arguments.clone()).unwrap();
        arguments["undeclared"] = true.into();
        let body = gateway
            .rpc("tools/call", json!({"name":"run","arguments":arguments}))
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
        let cases = input_fixture::ToolInputCase::load(include_bytes!(
            "../../../testdata/controlled-inputs.json"
        ));
        assert_eq!(cases.len(), 1);
        for case in cases {
            match case.tool.as_str() {
                "run" => {
                    let _: veoveo_media_mcp::contract::RunArgs = case.decode();
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
        assert!(state.tasks.list().await.unwrap().is_empty());
        let registry_after = state.registry_snapshot().await.unwrap();
        assert!(
            Arc::ptr_eq(&registry_before, &registry_after),
            "invalid inputs must preserve the admitted registry snapshot"
        );
        assert_eq!(
            state
                .registry_install_attempts
                .load(std::sync::atomic::Ordering::SeqCst),
            installs_before
        );
        let mut response =
            db.a.client()
                .query(include_str!("queries/invalid_input_state.surql"))
                .await
                .unwrap()
                .check()
                .unwrap();
        let storage_after: surrealdb::types::Value = response.take(0).unwrap();
        assert!(
            storage_before == storage_after,
            "invalid inputs must not mutate retained task, generation or provider state"
        );
    })
    .await
    .expect("Media argument admission exceeded 120 seconds");
}

// Aborting on unwind keeps this existing hosted test's only socket fixture owned.
struct BillingServer(tokio::task::JoinHandle<()>);
impl Drop for BillingServer {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
async fn signed_callback_requires_dispatch_binding_after_private_context_prune() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = fixture::TestDb::with_modules(vec![
            veoveo_media_mcp::schema::module_setup(
                fixture::module_lanes::execution("media").unwrap(),
            ).unwrap(),
        ]).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let billing_address = listener.local_addr().unwrap();
        let publication_rejections = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let rejection_counter = publication_rejections.clone();
        let billing_routes = Router::new().route("/api/v3/billings/search", post(|axum::Json(request): axum::Json<Value>| async move {
            let prediction = request["prediction_uuids"][0].as_str().unwrap();
            axum::Json(json!({"code":200,"message":"ok","data":{"items":[{
                "uuid":format!("billing-{prediction}"),"billing_type":"deduct","price":0.25,
                "prediction":{"uuid":prediction,"model_uuid":"fixture/image","status":if prediction == "callback-prediction" { "failed" } else { "completed" }}
            }]}}))
        })).route("/output", axum::routing::get(|| async {
            ([("content-type", "image/png")], "fixture-output-bytes")
        })).fallback(move || {
            let counter = rejection_counter.clone();
            async move {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                StatusCode::SERVICE_UNAVAILABLE
            }
        });
        let billing_server = BillingServer(tokio::spawn(async move {
            axum::serve(listener, billing_routes).await.unwrap();
        }));
        let args = config::Args::try_parse_from([
            "server",
            "--public-base-url",
            "https://media.test",
            "--api-key",
            "fixture",
            "--webhook-secret",
            "fixture",
            "--surreal-endpoint",
            "ws://127.0.0.1:9/rpc",
            "--surreal-namespace",
            "fixture",
            "--surreal-database",
            "fixture",
            "--surreal-auth-level",
            "database",
            "--surreal-username",
            "fixture",
            "--surreal-password",
            "fixture",
            "--internal-trust-jwks",
            r#"{"keys":[]}"#,
        ])
        .unwrap();
        let state = Arc::new(AppState {
            provider: veoveo_media_mcp::provider::ProviderClient::new("fixture")
                .with_base(format!("http://{billing_address}"))
                .unwrap(),
            http: reqwest::Client::new(),
            public_endpoint: veoveo_mcp_contract::PublicDeployment::new("https://media.test")
                .unwrap()
                .server("media")
                .unwrap(),
            webhook_secret: secrecy::SecretString::from("fixture"),
            registry: tokio::sync::RwLock::new(None),
            registry_install_attempts: Default::default(),
            tasks: veoveo_media_mcp::task_lookup::bind(TaskRuntime::new(db.a.clone(), "media", "callback-admission")).unwrap(),
            durable: veoveo_media_mcp::state::MediaState::new(db.a.clone()),
            artifacts: ArtifactRepository::new(format!("http://{billing_address}")),
            retention: args.retention_policy(),
            subscribers: SubscriptionHub::new(),
        });

        let handler = state.clone();
        let gateway = TestGateway::new(testing::for_domain::<MediaMcp>()
            .handler(move || Hosted::new(MediaMcp::new(handler.clone())))
            .public_routes(Router::new().route("/webhooks/{task_id}", post(media_webhook)).with_state(state.clone()))
            .build());
        let principal = testing::principal();
        let owner = veoveo_task_runtime::TaskOwner {
            principal_key: principal.id.to_string(),
            principal_kind: veoveo_task_runtime::PrincipalKind::User,
            issuer: principal.issuer.to_string(), subject: principal.subject.to_string(),
            profile: "operations".into(), tenant_key: Some("tenant-a".into()),
            data_labels: Default::default(), authority: testing::authority(),
        };
        let provider = veoveo_types::ExtensionName::parse("media").unwrap();
        let mut prepared = Vec::new();
        for _ in 0..3 {
            let task_id = TaskId::new();
            let snapshot = state.tasks.create(DurableCreateTask {
                task_id, owner: owner.clone(), server: "media".into(),
                task_type: const { veoveo_types::TaskTypeName::from_static("run") },
                request: json!({"model":"fixture/image","input":{}}),
                recovery_class: RecoveryClass::WebhookWait, idempotency_key: None,
                ttl_ms: Some(60_000), poll_interval_ms: Some(100), retention_pins: Default::default(),
            }).await.unwrap().snapshot;
            let capability = veoveo_artifact_contract::IssuedArtifactWriteCapability {
                capability_id: veoveo_artifact_contract::ArtifactWriteCapabilityId::new(),
                secret: veoveo_artifact_contract::ArtifactWriteCapabilitySecret::new("s".repeat(32)).unwrap(),
                task_id: veoveo_artifact_contract::ArtifactTaskId::try_from(task_id.as_uuid()).unwrap(), expires_at: Utc::now() + TimeDelta::hours(1),
            };
            state.durable.persist_task_context(&snapshot, &capability).await.unwrap();
            let binding = webhook::CallbackBinding::derive(&capability.secret, task_id, &owner.authority.tenant, &provider);
            state.tasks.claim(task_id, Duration::from_secs(30)).await.unwrap();
            let running = state.tasks.transition(task_id, TaskTransition::Running {
                message: "prepared fixture".into(), progress: 0.0,
            }).await.unwrap();
            assert_eq!(state.tasks.webhooks(provider.clone()).prepare_dispatch(task_id,
                veoveo_media_mcp::task_lookup::dispatch(&running, binding.digest()).unwrap()
            ).await.unwrap(), veoveo_task_runtime::DispatchPreparation::NewlyPrepared);
            assert!(state.durable.provider_job_for_task(task_id).await.unwrap().is_none());
            prepared.push((task_id, binding));
        }
        assert_ne!(prepared[0].1.digest(), prepared[1].1.digest());
        let body = serde_json::to_vec(&json!({"id":"callback-prediction","model":"fixture/image",
            "status":"failed","outputs":[],"error":"fixture provider failed"})).unwrap();
        let timestamp = Utc::now().timestamp().to_string();
        let signature = webhook::sign("fixture", "callback-event", &timestamp, &body);
        let callback_path = |task: TaskId, binding: &webhook::CallbackBinding| {
            let mut uri = reqwest::Url::parse("https://media.test").unwrap();
            uri.path_segments_mut().unwrap().extend(["webhooks", &task.to_string()]);
            uri.query_pairs_mut().append_pair("binding", binding.expose_secret());
            format!("{}?{}", uri.path(), uri.query().unwrap())
        };
        let signed = |path: &str| gateway.request(path).method("POST")
            .header("webhook-id", "callback-event").header("webhook-timestamp", &timestamp)
            .header("webhook-signature", &signature).body(axum::body::Body::from(body.clone())).unwrap();
        let rejected = gateway.send(signed(&callback_path(prepared[1].0, &prepared[0].1))).await;
        assert_eq!(rejected, (StatusCode::UNAUTHORIZED, "invalid binding".into()));
        let mut response = state.tasks.platform_store().client().query(include_str!("queries/callback_unsettled.surql"))
            .bind(("tasks", prepared.iter().map(|(task, _)| veoveo_platform_store::task_record_id(*task)).collect::<Vec<_>>()))
            .await.unwrap().check().unwrap();
        let changed = response.take::<Option<u64>>(0).unwrap().expect("callback mutation count");
        assert_eq!(changed, 0, "rejected callback must not associate or settle either Task");
        for (task, _) in &prepared {
            let snapshot = state.tasks.get(*task).await.unwrap().unwrap();
            assert_eq!(snapshot.status, veoveo_platform_store::TaskStatus::Waiting);
            assert!(state.durable.provider_job_for_task(*task).await.unwrap().is_none());
        }
        let unsigned = gateway.send(gateway.request(&format!("/webhooks/{}", prepared[0].0))
            .method("POST").body(axum::body::Body::from(body.clone())).unwrap()).await;
        assert_eq!(unsigned, (StatusCode::UNAUTHORIZED, "invalid signature".into()));
        state.tasks.platform_store().client().query(include_str!("queries/callback_expire_contexts.surql"))
            .bind(("tasks", prepared.iter().take(2).map(|(task, _)| veoveo_platform_store::task_record_id(*task)).collect::<Vec<_>>()))
            .await.unwrap().check().unwrap();
        assert_eq!(state.durable.prune_task_contexts().await.unwrap(), 2);
        for (task, _) in prepared.iter().take(2) {
            assert!(state.durable.task_context(&state.tasks.get(*task).await.unwrap().unwrap()).await.unwrap().is_none());
        }
        let accepted = gateway.send(signed(&callback_path(prepared[0].0, &prepared[0].1))).await;
        assert_eq!(accepted, (StatusCode::ACCEPTED, "accepted".into()));
        let failed = state.tasks.get(prepared[0].0).await.unwrap().unwrap();
        assert_eq!(failed.status, veoveo_platform_store::TaskStatus::Failed);
        assert_eq!(failed.error.as_ref().unwrap().code, "provider_failed");
        assert!(failed.result.is_none());
        assert!(failed.result_uri.is_none());
        assert!(state.durable.provider_job_for_task(prepared[0].0).await.unwrap().is_some());
        // Wait for the unchanged production billing worker to finish before closing its socket.
        tokio::time::timeout(Duration::from_secs(30), async {
            let pin = TaskRetentionPin::new("provider:media:webhook").unwrap();
            loop {
                if !state.tasks.get(prepared[0].0).await.unwrap().unwrap().retention_pins.contains(&pin) { break; }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.expect("loopback billing did not release the provider pin");
        assert!(state.durable.has_actual_usage(prepared[0].0, &"callback-prediction".parse().unwrap()).await.unwrap());
        // A valid private context reaches the Artifact client and receives a deliberate rejection.
        assert!(state.durable.task_context(&state.tasks.get(prepared[2].0).await.unwrap().unwrap()).await.unwrap().is_some());
        let publish_body = serde_json::to_vec(&json!({"id":"callback-prediction-3","model":"fixture/image","status":"completed","outputs":[format!("http://{billing_address}/output")]})).unwrap();
        let publish_signature = webhook::sign("fixture", "callback-publish", &timestamp, &publish_body);
        let accepted = gateway.send(gateway.request(&callback_path(prepared[2].0, &prepared[2].1)).method("POST")
            .header("webhook-id", "callback-publish").header("webhook-timestamp", &timestamp)
            .header("webhook-signature", &publish_signature).body(axum::body::Body::from(publish_body)).unwrap()).await;
        assert_eq!(accepted, (StatusCode::ACCEPTED, "accepted".into()));
        assert!(publication_rejections.load(std::sync::atomic::Ordering::SeqCst) > 0, "valid-context completion must reach the deliberately rejecting Artifact service");
        let failed = state.tasks.get(prepared[2].0).await.unwrap().unwrap();
        assert_eq!(failed.status, veoveo_platform_store::TaskStatus::Failed);
        assert_eq!(failed.error.unwrap().code, "artifact_publish_failed");
        assert!(failed.result.is_none() && failed.result_uri.is_none());
        assert_eq!(state.durable.provider_job_for_task(prepared[2].0).await.unwrap().unwrap().prediction.status, "completed");
        tokio::time::timeout(Duration::from_secs(30), async {
            let pin = TaskRetentionPin::new("provider:media:webhook").unwrap();
            while state.tasks.get(prepared[2].0).await.unwrap().unwrap().retention_pins.contains(&pin) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.expect("provider success billing did not release pin after Artifact rejection");
        assert!(state.durable.has_actual_usage(prepared[2].0, &"callback-prediction-3".parse().unwrap()).await.unwrap());
        // The second authenticated provider success survives local publication failure.
        let success_body = serde_json::to_vec(&json!({"id":"callback-prediction-2","model":"fixture/image","status":"completed","outputs":[]})).unwrap();
        let success_signature = webhook::sign("fixture", "callback-success", &timestamp, &success_body);
        let accepted = gateway.send(gateway.request(&callback_path(prepared[1].0, &prepared[1].1)).method("POST")
            .header("webhook-id", "callback-success").header("webhook-timestamp", &timestamp)
            .header("webhook-signature", &success_signature).body(axum::body::Body::from(success_body)).unwrap()).await;
        assert_eq!(accepted, (StatusCode::ACCEPTED, "accepted".into()));
        let local = state.tasks.get(prepared[1].0).await.unwrap().unwrap();
        assert_eq!(local.status, veoveo_platform_store::TaskStatus::Failed, "missing private write context is a local publication failure");
        assert_eq!(local.error.unwrap().code, "artifact_publish_failed");
        let provider_job = state.durable.provider_job_for_task(prepared[1].0).await.unwrap().unwrap();
        assert_eq!(provider_job.prediction.status, "completed", "local publication cannot replace authenticated provider success");
        tokio::time::timeout(Duration::from_secs(30), async {
            let pin = TaskRetentionPin::new("provider:media:webhook").unwrap();
            while state.tasks.get(prepared[1].0).await.unwrap().unwrap().retention_pins.contains(&pin) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.expect("successful provider billing did not release pin after local failure");
        assert!(state.durable.has_actual_usage(prepared[1].0, &"callback-prediction-2".parse().unwrap()).await.unwrap());
        billing_server.0.abort();
    }).await.expect("Media callback admission exceeded 120 seconds");
}

#[tokio::test]
async fn find_model_refresh_uses_the_captured_admitted_snapshot() {
    use axum::{Json, Router, extract::State, routing::get};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::{Mutex, Semaphore};
    tokio::time::timeout(Duration::from_secs(120), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = fixture::TestDb::with_modules(vec![
            veoveo_media_mcp::schema::module_setup(
                fixture::module_lanes::execution("media").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let models = ["fixture/a", "fixture/b"]
            .into_iter()
            .map(|id| veoveo_media_mcp::contract::ModelEntry {
                model_id: id.parse().unwrap(),
                name: id.into(),
                model_type: "image".into(),
                description: "fixture".into(),
                base_price: None,
                formula: None,
                api_schema: None,
            })
            .collect::<Vec<_>>();
        struct Replies {
            calls: AtomicUsize,
            gates: [Semaphore; 2],
            models: Mutex<[Vec<veoveo_media_mcp::contract::ModelEntry>; 2]>,
        }
        async fn reply(State(replies): State<Arc<Replies>>) -> Json<serde_json::Value> {
            let index = replies.calls.fetch_add(1, Ordering::SeqCst) % 2;
            replies.gates[index].acquire().await.unwrap().forget();
            Json(json!({"code":200,"data":replies.models.lock().await[index]}))
        }
        let replies = Arc::new(Replies {
            calls: AtomicUsize::new(0),
            gates: [Semaphore::new(0), Semaphore::new(0)],
            models: Mutex::new([models.clone(), vec![]]),
        });
        let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let server_replies = replies.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                socket,
                Router::new()
                    .route("/api/v3/models", get(reply))
                    .with_state(server_replies),
            )
            .await
            .unwrap();
        });
        struct Server(tokio::task::JoinHandle<()>);
        impl Drop for Server {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let _server = Server(server);
        let args = config::Args::try_parse_from([
            "server",
            "--public-base-url",
            "https://media.test",
            "--api-key",
            "fixture",
            "--webhook-secret",
            "fixture",
            "--surreal-endpoint",
            "ws://127.0.0.1:9/rpc",
            "--surreal-namespace",
            "fixture",
            "--surreal-database",
            "fixture",
            "--surreal-auth-level",
            "database",
            "--surreal-username",
            "fixture",
            "--surreal-password",
            "fixture",
            "--internal-trust-jwks",
            r#"{"keys":[]}"#,
        ])
        .unwrap();
        let state = Arc::new(AppState {
            provider: veoveo_media_mcp::provider::ProviderClient::new("fixture")
                .with_base(format!("http://{address}"))
                .unwrap(),
            http: reqwest::Client::new(),
            public_endpoint: veoveo_mcp_contract::PublicDeployment::new("https://media.test")
                .unwrap()
                .server("media")
                .unwrap(),
            webhook_secret: secrecy::SecretString::from("fixture"),
            registry: tokio::sync::RwLock::new(None),
            registry_install_attempts: Default::default(),
            tasks: TaskRuntime::new(db.a.clone(), "media", "strict-input"),
            durable: veoveo_media_mcp::state::MediaState::new(db.a.clone()),
            artifacts: ArtifactRepository::new("http://127.0.0.1:9"),
            retention: args.retention_policy(),
            subscribers: SubscriptionHub::new(),
        });

        for second in [
            vec![models[1].clone(), models[0].clone()],
            vec![models[1].clone()],
        ] {
            *replies.models.lock().await = [models.clone(), second];
            replies.calls.store(0, Ordering::SeqCst);
            state.registry_install_attempts.store(0, Ordering::SeqCst);
            *state.registry.write().await = None;
            // Existing read permits allow both cold fetches but prevent either install.
            let held = state.registry.read().await;
            let a = state.clone();
            let requested = models[1].model_id.clone();
            let lookup = tokio::spawn(async move { a.find_model(&requested).await });
            tokio::time::timeout(Duration::from_secs(10), async {
                while replies.calls.load(Ordering::SeqCst) < 1 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let b = state.clone();
            let refresh = tokio::spawn(async move { b.registry().await });
            tokio::time::timeout(Duration::from_secs(10), async {
                while replies.calls.load(Ordering::SeqCst) < 2 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            replies.gates[0].add_permits(1);
            tokio::time::timeout(Duration::from_secs(10), async {
                while state.registry_install_attempts.load(Ordering::SeqCst) < 1 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            replies.gates[1].add_permits(1);
            tokio::time::timeout(Duration::from_secs(10), async {
                while state.registry_install_attempts.load(Ordering::SeqCst) < 2 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            // Tokio's fair lock queues refresh B before any second read by lookup A.
            drop(held);
            assert_eq!(
                lookup.await.unwrap().unwrap().unwrap().model_id,
                models[1].model_id
            );
            assert_eq!(
                refresh.await.unwrap().unwrap().len(),
                replies.models.lock().await[1].len()
            );
            assert_eq!(
                state
                    .find_model(&models[1].model_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .model_id,
                models[1].model_id
            );
            assert!(state.tasks.list().await.unwrap().is_empty());
        }
    })
    .await
    .expect("Media snapshot refresh control exceeded 120 seconds");
}
