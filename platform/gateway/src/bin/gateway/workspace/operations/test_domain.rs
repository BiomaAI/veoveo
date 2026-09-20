//! Shared native Task fixture for browser-projection and model-tool acceptance.
use super::*;
use futures::StreamExt;
use rmcp::{
    RoleServer, ServerHandler,
    model::*,
    service::{RequestContext, SubscriptionContext},
    transport::streamable_http_server::StreamableHttpService,
};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use veoveo_task_runtime::{
    CreateTask, RecoveryClass, TaskId, TaskInputRequest, TaskOwner, TaskRuntime,
};

#[derive(Clone)]
pub(crate) struct Domain {
    pub runtime: TaskRuntime,
    pub owner: TaskOwner,
    pub calls: Arc<AtomicUsize>,
    pub app_visible: Arc<AtomicBool>,
    pub hold_dispatch: Arc<AtomicBool>,
    pub release_dispatch: Arc<tokio::sync::Notify>,
    pub degraded_server: Arc<std::sync::Mutex<Option<veoveo_mcp_contract::ServerSlug>>>,
    pub reactive_catalog: Arc<AtomicBool>,
    pub catalog_reads: Arc<AtomicUsize>,
    pub catalog_requested: Arc<tokio::sync::Notify>,
    pub catalog_epoch: tokio::sync::watch::Sender<u64>,
}
impl ServerHandler for Domain {
    fn supported_protocol_versions(&self) -> std::borrow::Cow<'static, [ProtocolVersion]> {
        std::borrow::Cow::Owned(vec![ProtocolVersion::V_2026_07_28])
    }
    fn get_info(&self) -> ServerConfig {
        let mut config = ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_tasks()
                .build(),
        );
        if self.reactive_catalog.load(Ordering::SeqCst) {
            config.capabilities.tools.as_mut().unwrap().list_changed = Some(true);
        }
        config
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let mut result = ListToolsResult::with_all_items(vec![
            veoveo_mcp_apps_extension::link_tool_to_app(
                Tool::new(
                    "fixture__task",
                    "Native App Task fixture",
                    serde_json::from_value::<JsonObject>(json!({"type":"object","properties":{}}))
                        .unwrap(),
                ),
                "ui://fixture/task.html",
                &[veoveo_mcp_apps_extension::UiVisibility::App],
            ),
            Tool::new(
                "fixture__task",
                "Create an explicit durable Task fixture",
                serde_json::from_value::<JsonObject>(json!({"type":"object","properties":{}}))
                    .unwrap(),
            ),
        ]);
        if let Some(server) = self.degraded_server.lock().unwrap().clone() {
            result.meta = veoveo_mcp_contract::GatewayDiscoveryDegradation::new([
                veoveo_mcp_contract::GatewayDiscoveryFailure {
                    server,
                    surface: veoveo_mcp_contract::GatewayDiscoverySurface::Tools,
                    code: veoveo_mcp_contract::GatewayDiscoveryFailureCode::UpstreamUnavailable,
                },
            ])
            .into_meta();
        }
        self.catalog_reads.fetch_add(1, Ordering::SeqCst);
        self.catalog_requested.notify_one();
        Ok(result)
    }
    async fn list_resources(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(
            if self.app_visible.load(Ordering::SeqCst) {
                vec![veoveo_mcp_apps_extension::app_resource(
                    "ui://fixture/task.html",
                    "Task fixture",
                )]
            } else {
                vec![]
            },
        ))
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        assert!(
            context
                .meta
                .client_capabilities()
                .is_some_and(|capabilities| capabilities.supports_tasks()),
            "native invocation must carry the negotiated Tasks capability"
        );
        assert!(request.name.as_ref() == "fixture__task");
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(token) = context.meta.get_progress_token() {
            context
                .peer
                .notify_progress(
                    ProgressNotificationParam::new(token, 4.0)
                        .with_total(10.0)
                        .with_message("Measured fixture work"),
                )
                .await
                .unwrap();
        }
        if self.hold_dispatch.load(Ordering::SeqCst) {
            self.release_dispatch.notified().await;
        }
        if request
            .arguments
            .as_ref()
            .is_some_and(|args| args.get("mode") == Some(&json!("mrtr")))
        {
            if let Some(state) = request.request_state {
                assert_eq!(state, "protected-fixture-continuation");
                assert_eq!(
                    request.input_responses.unwrap()["confirm"]["action"],
                    "accept"
                );
                return Ok(CallToolResult::success(vec![ContentBlock::text(
                    "Confirmed continuation",
                )])
                .into());
            }
            let input: InputRequest = serde_json::from_value(json!({"method":"elicitation/create","params":{"mode":"form","message":"Continue this operation?","requestedSchema":{"type":"object","properties":{"approved":{"type":"boolean"}},"required":["approved"]}}})).unwrap();
            return Ok(InputRequiredResult::new(
                Some(std::collections::BTreeMap::from([(
                    "confirm".into(),
                    input,
                )])),
                Some("protected-fixture-continuation".into()),
            )
            .into());
        }
        assert!(request.request_state.is_none());
        let task = self
            .runtime
            .create(CreateTask {
                task_id: TaskId::new(),
                owner: self.owner.clone(),
                server: "workspace-fixture".into(),
                task_type: "workspace-acceptance".into(),
                request: json!({}),
                recovery_class: RecoveryClass::InterruptedIndeterminate,
                idempotency_key: None,
                ttl_ms: Some(300_000),
                poll_interval_ms: Some(1000),
                retention_pins: Default::default(),
            })
            .await
            .unwrap()
            .snapshot;
        let id = task.task_id.to_string();
        self.runtime
            .claim(&id, Duration::from_secs(60))
            .await
            .unwrap();
        self.runtime.request_input(&id, "approval-1", TaskInputRequest {
            method: "elicitation/create".into(), params: serde_json::from_value(json!({"mode":"form","message":"Choose a count for the fixture.",
                "requestedSchema":{"type":"object","properties":{"count":{"type":"integer","minimum":1,"maximum":3}},"required":["count"]}})).unwrap(),
        }).await.unwrap();
        Ok(CallToolResponse::Task(rmcp::model::CreateTaskResult::new(
            veoveo_task_runtime::task_seed(&task),
        )))
    }
    async fn get_task(
        &self,
        request: GetTaskParams,
        _: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, ErrorData> {
        veoveo_task_runtime::get_durable_task(&self.runtime, &self.owner, request).await
    }
    async fn update_task(
        &self,
        request: UpdateTaskParams,
        _: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        veoveo_task_runtime::update_durable_task(&self.runtime, &self.owner, request).await
    }
    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        _: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        veoveo_task_runtime::cancel_durable_task(&self.runtime, &self.owner, request.task_id).await
    }
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        Some(requested.clone())
    }
    async fn listen(&self, context: SubscriptionContext) -> Result<(), ErrorData> {
        if context.accepted().tools_list_changed == Some(true) {
            let mut changes = self.catalog_epoch.subscribe();
            loop {
                tokio::select! {
                    _ = context.cancelled() => return Ok(()),
                    change = changes.changed() => {
                        if change.is_err() { return Ok(()); }
                        context.sink().notify_tool_list_changed().await
                            .map_err(|_| ErrorData::internal_error("catalog stream ended", None))?;
                    }
                }
            }
        }
        let mut source = veoveo_task_runtime::subscribe_durable_tasks(
            &self.runtime,
            self.owner.clone(),
            context.accepted().task_ids.clone().unwrap_or_default(),
        )
        .await?
        .updates;
        loop {
            tokio::select! {
                _ = context.cancelled() => return Ok(()),
                value = source.next() => match value {
                    Some(Ok(task)) => context.sink().notify_task_status(task).await.map_err(|_| ErrorData::internal_error("fixture stream ended", None))?,
                    _ => return Ok(()),
                }
            }
        }
    }
}

pub(crate) struct Fixture {
    pub domain: Domain,
    pub port: u16,
    server: tokio::task::AbortHandle,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    pub async fn start(store: PlatformStore, subject: &AuthenticatedSubject) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let owner = TaskOwner {
            principal_key: subject.principal.id.to_string(),
            principal_kind: veoveo_task_runtime::PrincipalKind::User,
            issuer: subject.principal.issuer.to_string(),
            subject: subject.principal.subject.to_string(),
            profile: "operator".into(),
            tenant_key: Some("test".into()),
            data_labels: Default::default(),
            authority: subject.authority.clone(),
        };
        let domain = Domain {
            runtime: TaskRuntime::new(store, "workspace-fixture", "fixture-worker"),
            owner,
            calls: Arc::new(AtomicUsize::new(0)),
            app_visible: Arc::new(AtomicBool::new(true)),
            hold_dispatch: Arc::new(AtomicBool::new(false)),
            release_dispatch: Arc::new(tokio::sync::Notify::new()),
            degraded_server: Arc::new(std::sync::Mutex::new(None)),
            reactive_catalog: Arc::new(AtomicBool::new(false)),
            catalog_reads: Arc::new(AtomicUsize::new(0)),
            catalog_requested: Arc::new(tokio::sync::Notify::new()),
            catalog_epoch: tokio::sync::watch::channel(0).0,
        };
        let source = domain.clone();
        let service = StreamableHttpService::new(
            move || Ok(source.clone()),
            veoveo_mcp_contract::stateless_session_manager(),
            veoveo_mcp_contract::canonical_streamable_http_server_config()
                .with_allowed_hosts(["workspace.test"]),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(
            axum::serve(
                listener,
                Router::new().nest_service("/mcp/operator", service).layer(
                    axum::middleware::from_fn(
                        |headers: HeaderMap,
                         request: axum::extract::Request,
                         next: axum::middleware::Next| async move {
                            assert_eq!(
                                headers.get("authorization").unwrap(),
                                "Bearer explicit-workspace-fixture"
                            );
                            assert_eq!(headers.get("host").unwrap(), "workspace.test");
                            next.run(request).await
                        },
                    ),
                ),
            )
            .into_future(),
        )
        .abort_handle();
        Self {
            domain,
            port,
            server,
        }
    }
}
