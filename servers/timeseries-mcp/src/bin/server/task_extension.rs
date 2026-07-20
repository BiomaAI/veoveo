use std::collections::BTreeSet;
use std::sync::Arc;

use futures::StreamExt;
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};
use veoveo_mcp_task_extension::{
    AcknowledgeTaskResult, AdapterError, CancelTaskParams, CreateTaskResult, GetTaskParams,
    GetTaskResult, ProtocolTaskId, TaskExtensionHandler, TaskSubscription, ToolCallParams,
    UpdateTaskParams, project_snapshot, task_seed,
};
use veoveo_task_runtime::TaskSnapshot;
use veoveo_timeseries_mcp::contract::TimeseriesForecastRequest;

use super::app_state::AppState;
use super::internal_auth::ForwardedBearer;
use super::ownership::{caller_from, runtime_owner};
use super::start_forecast_task;

#[derive(Clone)]
pub(super) struct TimeseriesTaskExtension {
    state: Arc<AppState>,
}

impl TimeseriesTaskExtension {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    async fn authorized_snapshot(
        &self,
        caller: &AuthenticatedCaller,
        task_id: ProtocolTaskId,
    ) -> Result<TaskSnapshot, AdapterError> {
        let snapshot = self
            .state
            .tasks
            .get(&task_id.to_string())
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?
            .ok_or_else(|| AdapterError::invalid_params("unknown task id"))?;
        let caller_owner = runtime_owner(&caller.identity);
        if snapshot.owner.allows(
            &caller_owner.principal_key,
            &caller_owner.profile,
            caller_owner.tenant_key.as_deref(),
            &caller_owner.data_labels,
        ) {
            Ok(snapshot)
        } else {
            Err(AdapterError::invalid_params("unknown task id"))
        }
    }
}

#[derive(Clone)]
pub(super) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
    plane: PlaneCaller,
}

impl TaskExtensionHandler for TimeseriesTaskExtension {
    type Caller = AuthenticatedCaller;

    fn authenticate(
        &self,
        extensions: &axum::http::Extensions,
    ) -> Result<Self::Caller, AdapterError> {
        let identity = extensions
            .get::<GatewayInternalIdentity>()
            .cloned()
            .ok_or_else(|| AdapterError::unauthorized("gateway identity missing"))?;
        let bearer = extensions
            .get::<ForwardedBearer>()
            .map(|bearer| bearer.0.clone())
            .ok_or_else(|| AdapterError::unauthorized("forwarded bearer missing"))?;
        Ok(AuthenticatedCaller {
            plane: caller_from(identity.clone(), bearer),
            identity,
        })
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: ToolCallParams,
    ) -> Result<Option<CreateTaskResult>, AdapterError> {
        if request.name != "forecast" {
            return Ok(None);
        }
        let retention_pins = request.meta.task_retention_pin.into_iter().collect();
        let args: TimeseriesForecastRequest = serde_json::from_value(serde_json::Value::Object(
            request.arguments.into_iter().collect(),
        ))
        .map_err(|error| AdapterError::invalid_params(error.to_string()))?;
        let snapshot = start_forecast_task(
            self.state.clone(),
            caller.identity.clone(),
            caller.plane.clone(),
            args,
            None,
            retention_pins,
        )
        .await
        .map_err(AdapterError::internal)?;
        Ok(Some(CreateTaskResult::new(task_seed(&snapshot))))
    }

    async fn get_task(
        &self,
        caller: &Self::Caller,
        request: GetTaskParams,
    ) -> Result<GetTaskResult, AdapterError> {
        let snapshot = self.authorized_snapshot(caller, request.task_id).await?;
        let task = project_snapshot(&self.state.tasks, snapshot)
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?;
        Ok(GetTaskResult::new(task))
    }

    async fn update_task(
        &self,
        caller: &Self::Caller,
        request: UpdateTaskParams,
    ) -> Result<AcknowledgeTaskResult, AdapterError> {
        self.authorized_snapshot(caller, request.task_id).await?;
        self.state
            .tasks
            .submit_input_responses(&request.task_id.to_string(), request.input_responses)
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?;
        Ok(AcknowledgeTaskResult::complete())
    }

    async fn cancel_task(
        &self,
        caller: &Self::Caller,
        request: CancelTaskParams,
    ) -> Result<AcknowledgeTaskResult, AdapterError> {
        self.authorized_snapshot(caller, request.task_id).await?;
        self.state
            .tasks
            .cancel(&request.task_id.to_string())
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?;
        Ok(AcknowledgeTaskResult::complete())
    }

    async fn subscribe_tasks(
        &self,
        caller: &Self::Caller,
        task_ids: Vec<ProtocolTaskId>,
    ) -> Result<TaskSubscription, AdapterError> {
        let updates = self
            .state
            .tasks
            .live_updates()
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?;
        let mut accepted = Vec::new();
        for task_id in task_ids {
            if self.authorized_snapshot(caller, task_id).await.is_ok() {
                accepted.push(task_id);
            }
        }
        let accepted_set: BTreeSet<_> = accepted.iter().copied().collect();
        let runtime = self.state.tasks.clone();
        let caller_owner = runtime_owner(&caller.identity);
        let stream = updates.filter_map(move |update| {
            let accepted = accepted_set.clone();
            let runtime = runtime.clone();
            let caller_owner = caller_owner.clone();
            async move {
                let snapshot = match update {
                    Ok(update) => update.snapshot,
                    Err(error) => return Some(Err(AdapterError::internal(error.to_string()))),
                };
                if !accepted.contains(&ProtocolTaskId::from(snapshot.task_id))
                    || !snapshot.owner.allows(
                        &caller_owner.principal_key,
                        &caller_owner.profile,
                        caller_owner.tenant_key.as_deref(),
                        &caller_owner.data_labels,
                    )
                {
                    return None;
                }
                Some(
                    project_snapshot(&runtime, snapshot)
                        .await
                        .map_err(|error| AdapterError::internal(error.to_string())),
                )
            }
        });
        Ok(TaskSubscription {
            accepted_task_ids: accepted,
            updates: Box::pin(stream),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};
    use std::sync::Arc as StdArc;
    use std::time::Duration;

    use axum::body::{Body, Bytes, to_bytes};
    use axum::extract::State;
    use axum::http::{HeaderMap, Request};
    use axum::{Json, Router, middleware, routing::post};
    use chrono::{TimeDelta, Utc};
    use secrecy::SecretString;
    use serde_json::{Value, json};
    use tower::ServiceExt;
    use veoveo_duckdb_runtime::HttpsSourcePolicy;
    use veoveo_mcp_contract::{
        AccessSubject, ArtifactId, ArtifactMetadata, ArtifactReleaseState,
        ArtifactWriteCapabilityId, ArtifactWriteCapabilitySecret, ComplianceMetadata, DataLabelId,
        GatewayInternalIdentity, GatewayProfileId, GroupId, InvocationAuthority,
        InvocationProvenance, IssueArtifactWriteCapabilityRequest, IssuedArtifactWriteCapability,
        JwtId, PolicyVersion, Principal, PrincipalAssurance, PrincipalId, PrincipalKind,
        RedeemArtifactWriteCapabilityRequest, RoleId, ScopeName, ServerSlug, TenantId, TokenIssuer,
        TokenSubject, WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
    };
    use veoveo_mcp_task_extension::{
        Implementation, PROTOCOL_VERSION, ServerDiscovery, TaskExtensionAdapter,
        task_extension_middleware,
    };
    use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials};
    use veoveo_task_runtime::{
        CreateTask as DurableCreateTask, RecoveryClass, TaskPayloadState, TaskRetentionPin,
        TaskRuntime,
    };
    use veoveo_timeseries_mcp::artifacts::ArtifactRepository;

    use super::*;
    use crate::{
        ForecastTaskRequest, MCP_TASK_POLL_INTERVAL_MS, MCP_TASK_TTL_MS, SERVER_SLUG,
        resume_forecast_task,
    };

    async fn replica_runtimes() -> Option<(TaskRuntime, TaskRuntime)> {
        if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
            return None;
        }
        let endpoint = std::env::var("VEOVEO_SURREAL_ENDPOINT")
            .unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
        let username =
            std::env::var("VEOVEO_SURREAL_USERNAME").unwrap_or_else(|_| "root".to_owned());
        let password =
            std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
        let config = StoreConfig::builder(
            endpoint,
            "veoveo_timeseries_task_test",
            format!("task_{}", uuid::Uuid::now_v7().simple()),
            StoreCredentials::root(username, SecretString::from(password)),
        )
        .migrate_on_connect(true)
        .build()
        .unwrap();
        let first_store = PlatformStore::connect(config.clone()).await.unwrap();
        let second_store = PlatformStore::connect(config).await.unwrap();
        Some((
            TaskRuntime::new(first_store, "timeseries", "integration-worker-a"),
            TaskRuntime::new(second_store, "timeseries", "integration-worker-b"),
        ))
    }

    async fn runtime() -> Option<TaskRuntime> {
        replica_runtimes().await.map(|(runtime, _)| runtime)
    }

    fn identity() -> GatewayInternalIdentity {
        let now = Utc::now();
        let actor = Principal {
            id: PrincipalId::new("integration-user").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://issuer.integration.example").unwrap(),
            subject: TokenSubject::new("integration-subject").unwrap(),
            tenant: Some(TenantId::new("integration-tenant").unwrap()),
            groups: BTreeSet::<GroupId>::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::<RoleId>::new(),
            scopes: BTreeSet::<ScopeName>::new(),
            data_labels: BTreeSet::<DataLabelId>::new(),
            assurances: BTreeSet::<PrincipalAssurance>::new(),
            authenticated_at: Some(now),
        };
        GatewayInternalIdentity {
            issuer: TokenIssuer::new("veoveo-internal").unwrap(),
            profile: GatewayProfileId::new("default").unwrap(),
            server: ServerSlug::new("timeseries").unwrap(),
            actor: actor.clone(),
            authority: InvocationAuthority {
                work_context: WorkContextId::new("integration-mission").unwrap(),
                tenant: TenantId::new("integration-tenant").unwrap(),
                membership: WorkContextMembershipLevel::Owner,
                policy_revision: PolicyVersion::new("r1").unwrap(),
                output_policy: WorkContextOutputPolicy {
                    owner: AccessSubject::Principal(actor.id.clone()),
                    initial_grants: Vec::new(),
                    classification: None,
                    data_labels: BTreeSet::new(),
                },
                provenance: InvocationProvenance::Direct {
                    initiator: actor.id,
                },
            },
            jwt_id: JwtId::new(uuid::Uuid::now_v7().to_string()).unwrap(),
            issued_at: now,
            not_before: now,
            expires_at: now + TimeDelta::minutes(5),
        }
    }

    #[derive(Clone, Debug)]
    struct CapturedRedemption {
        request: RedeemArtifactWriteCapabilityRequest,
        bytes: Vec<u8>,
    }

    #[derive(Clone)]
    struct FakeArtifactState {
        artifact_id: ArtifactId,
        created_at: chrono::DateTime<Utc>,
        redemptions: StdArc<tokio::sync::Mutex<Vec<CapturedRedemption>>>,
    }

    async fn artifact_service() -> (String, FakeArtifactState, tokio::task::JoinHandle<()>) {
        async fn issue_capability(
            Json(request): Json<IssueArtifactWriteCapabilityRequest>,
        ) -> Json<IssuedArtifactWriteCapability> {
            Json(IssuedArtifactWriteCapability {
                capability_id: ArtifactWriteCapabilityId::new(),
                secret: ArtifactWriteCapabilitySecret::new(
                    "integration-capability-secret-000000000000",
                )
                .unwrap(),
                task_id: request.task_id,
                expires_at: request.expires_at,
            })
        }

        async fn redeem_capability(
            State(state): State<FakeArtifactState>,
            headers: HeaderMap,
            body: Bytes,
        ) -> Json<ArtifactMetadata> {
            let request = serde_json::from_str::<RedeemArtifactWriteCapabilityRequest>(
                headers
                    .get("x-artifact-capability-redeem")
                    .unwrap()
                    .to_str()
                    .unwrap(),
            )
            .unwrap();
            state.redemptions.lock().await.push(CapturedRedemption {
                request,
                bytes: body.to_vec(),
            });
            Json(ArtifactMetadata {
                artifact_id: state.artifact_id,
                byte_len: body.len() as u64,
                mime_type: Some("application/vnd.rerun.rrd".to_owned()),
                filename: Some("forecast.rrd".to_owned()),
                artifact_uri: state.artifact_id.plane_uri(),
                download_url: None,
                created_at: state.created_at,
                release_state: ArtifactReleaseState::Private,
                compliance: ComplianceMetadata::default(),
                metadata: Value::Null,
            })
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let state = FakeArtifactState {
            artifact_id: ArtifactId::new(),
            created_at: Utc::now(),
            redemptions: StdArc::new(tokio::sync::Mutex::new(Vec::new())),
        };
        let server_state = state.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new()
                    .route("/artifact-write-capabilities", post(issue_capability))
                    .route(
                        "/artifact-write-capabilities/{capability_id}/redeem",
                        post(redeem_capability),
                    )
                    .with_state(server_state),
            )
            .await
            .unwrap();
        });
        (format!("http://{address}"), state, server)
    }

    fn request(method: &str, name: &str, params: Value) -> Request<Body> {
        let mut request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .header("mcp-protocol-version", PROTOCOL_VERSION)
            .header("mcp-method", method)
            .header("mcp-name", name)
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "jsonrpc": "2.0",
                    "id": "integration-request",
                    "method": method,
                    "params": params,
                }))
                .unwrap(),
            ))
            .unwrap();
        let identity = identity();
        request.extensions_mut().insert(identity);
        request
            .extensions_mut()
            .insert(ForwardedBearer("forwarded-token".to_owned()));
        request
    }

    fn meta() -> Value {
        json!({
            "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
            "io.modelcontextprotocol/clientCapabilities": {
                "extensions": {"io.modelcontextprotocol/tasks": {}}
            },
            "ai.bioma.veoveo/taskRetentionPin": "agent-episode:integration"
        })
    }

    async fn json_body(response: axum::response::Response) -> Value {
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn forecast_task_runs_end_to_end_through_final_extension() {
        let Some(tasks) = runtime().await else {
            return;
        };
        let (artifact_url, artifact_state, artifact_server) = artifact_service().await;
        let state = Arc::new(AppState {
            tasks: tasks.clone(),
            artifacts: ArtifactRepository::new(artifact_url),
            source_policy: HttpsSourcePolicy::deny_network(),
            max_artifact_bytes: 16 * 1024 * 1024,
        });
        let adapter = Arc::new(TaskExtensionAdapter::new(
            Arc::new(TimeseriesTaskExtension::new(state)),
            ServerDiscovery::new(
                HashMap::from([("tools".to_owned(), json!({}))])
                    .into_iter()
                    .collect(),
                Implementation {
                    name: "timeseries".to_owned(),
                    version: "test".to_owned(),
                },
                None,
            ),
        ));
        let app = Router::new()
            .fallback(|| async { Json(json!({"unexpected": true})) })
            .layer(middleware::from_fn_with_state(
                adapter,
                task_extension_middleware::<TimeseriesTaskExtension>,
            ));

        let response = app
            .clone()
            .oneshot(request(
                "tools/call",
                "forecast",
                json!({
                    "_meta": meta(),
                    "name": "forecast",
                    "arguments": {
                        "source": {
                            "kind": "inline_csv",
                            "csv": "ts,value\n2026-01-01,10\n2026-01-02,12\n2026-01-03,15\n",
                            "options": {"header": true}
                        },
                        "mapping": {"time_column": "ts", "value_column": "value"},
                        "horizon": 3,
                        "method": "naive_trend"
                    }
                }),
            ))
            .await
            .unwrap();
        let created = json_body(response).await;
        assert_eq!(created["result"]["resultType"], "task");
        let task_id = created["result"]["taskId"].as_str().unwrap().to_owned();
        let snapshot = tasks.get(&task_id).await.unwrap().unwrap();
        assert!(
            snapshot
                .retention_pins
                .contains(&TaskRetentionPin::new("agent-episode:integration").unwrap())
        );
        let persisted = serde_json::from_value::<ForecastTaskRequest>(snapshot.request).unwrap();
        assert_eq!(persisted.artifact_write_capability.task_id, task_id);
        assert!(matches!(
            tasks.await_payload_state(&task_id).await.unwrap(),
            TaskPayloadState::Completed(_)
        ));

        let response = app
            .clone()
            .oneshot(request(
                "tasks/get",
                &task_id,
                json!({"_meta": meta(), "taskId": task_id}),
            ))
            .await
            .unwrap();
        let completed = json_body(response).await;
        assert_eq!(completed["result"]["status"], "completed");
        assert_eq!(completed["result"]["result"]["isError"], false);
        assert!(
            completed["result"]["result"]["structuredContent"]["artifact"]["artifact_uri"]
                .as_str()
                .unwrap()
                .starts_with("timeseries://artifact/")
        );

        let response = app
            .clone()
            .oneshot(request(
                "tools/call",
                "forecast",
                json!({
                    "_meta": meta(),
                    "name": "forecast",
                    "arguments": {
                        "source": {
                            "kind": "inline_csv",
                            "csv": "ts,value\n2026-01-01,10\n2026-01-02,12\n",
                            "options": {"header": true}
                        },
                        "mapping": {"time_column": "missing", "value_column": "value"},
                        "horizon": 2,
                        "method": "naive_trend"
                    }
                }),
            ))
            .await
            .unwrap();
        let created = json_body(response).await;
        let failed_task_id = created["result"]["taskId"].as_str().unwrap().to_owned();
        assert!(matches!(
            tasks.await_payload_state(&failed_task_id).await.unwrap(),
            TaskPayloadState::Completed(_)
        ));
        let response = app
            .oneshot(request(
                "tasks/get",
                &failed_task_id,
                json!({"_meta": meta(), "taskId": failed_task_id}),
            ))
            .await
            .unwrap();
        let completed_error = json_body(response).await;
        assert_eq!(completed_error["result"]["status"], "completed");
        assert_eq!(completed_error["result"]["result"]["isError"], true);
        let redemptions = artifact_state.redemptions.lock().await;
        assert_eq!(redemptions.len(), 1);
        assert_eq!(
            redemptions[0].request.idempotency_key.as_str(),
            format!("timeseries:{task_id}:forecast")
        );
        assert_eq!(redemptions[0].request.task_id, task_id);
        assert!(!redemptions[0].bytes.is_empty());
        artifact_server.abort();
    }

    #[tokio::test]
    async fn expired_task_is_taken_over_by_second_replica_and_usage_is_shared() {
        let Some((first, second)) = replica_runtimes().await else {
            return;
        };
        let (artifact_url, artifact_state, artifact_server) = artifact_service().await;
        let artifacts = ArtifactRepository::new(artifact_url);
        let identity = identity();
        let task_id = veoveo_task_runtime::TaskId::new();
        let capability = artifacts
            .issue_write_capability(
                &caller_from(identity.clone(), "forwarded-token".to_owned()),
                &IssueArtifactWriteCapabilityRequest {
                    task_id: task_id.to_string(),
                    expires_at: Utc::now() + TimeDelta::hours(1),
                    max_artifact_count: std::num::NonZeroU32::new(1).unwrap(),
                    max_total_bytes: std::num::NonZeroU64::new(16 * 1024 * 1024).unwrap(),
                },
            )
            .await
            .unwrap();
        let request = ForecastTaskRequest {
            input: TimeseriesForecastRequest {
                source: veoveo_mcp_contract::DuckDbSource::InlineCsv {
                    csv: "ts,value\n2026-01-01,10\n2026-01-02,12\n2026-01-03,15\n".into(),
                    filename: Some("takeover.csv".into()),
                    options: veoveo_mcp_contract::DuckDbReadOptions {
                        header: Some(true),
                        ..Default::default()
                    },
                },
                mapping: veoveo_timeseries_mcp::contract::TimeseriesTableMapping {
                    time_column: Some("ts".into()),
                    value_column: "value".into(),
                    series_column: None,
                },
                training_filter: None,
                horizon: 2,
                method: veoveo_timeseries_mcp::contract::TimeseriesForecastMethod::NaiveTrend,
            },
            artifact_write_capability: capability,
        };
        let created = first
            .create(DurableCreateTask {
                task_id,
                owner: runtime_owner(&identity),
                server: SERVER_SLUG.to_owned(),
                task_type: "forecast".to_owned(),
                request: serde_json::to_value(&request).unwrap(),
                recovery_class: RecoveryClass::Resume,
                idempotency_key: None,
                ttl_ms: Some(MCP_TASK_TTL_MS),
                poll_interval_ms: Some(MCP_TASK_POLL_INTERVAL_MS),
                retention_pins: BTreeSet::new(),
            })
            .await
            .unwrap();
        first
            .claim(&task_id.to_string(), Duration::from_millis(10))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        let recovery = second.recover().await.unwrap();
        let snapshot = recovery
            .resumable
            .into_iter()
            .find(|snapshot| snapshot.task_id == task_id)
            .expect("expired task is resumable on replica B");
        let state = Arc::new(AppState {
            tasks: second.clone(),
            artifacts,
            source_policy: HttpsSourcePolicy::deny_network(),
            max_artifact_bytes: 16 * 1024 * 1024,
        });
        resume_forecast_task(state, snapshot).await.unwrap();
        assert!(matches!(
            second
                .await_payload_state(&task_id.to_string())
                .await
                .unwrap(),
            TaskPayloadState::Completed(_)
        ));

        let usage = first
            .platform_store()
            .domain_usage_for_task(SERVER_SLUG, task_id)
            .await
            .unwrap();
        assert_eq!(usage.len(), 1);
        assert_eq!(usage[0].model_id, "timeseries/naive-trend");
        assert_eq!(artifact_state.redemptions.lock().await.len(), 1);
        assert_eq!(created.snapshot.task_id, task_id);
        artifact_server.abort();
    }
}
