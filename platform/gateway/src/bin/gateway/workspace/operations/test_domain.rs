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
use std::sync::atomic::{AtomicUsize, Ordering};
use veoveo_task_runtime::{
    CreateTask, RecoveryClass, TaskId, TaskInputRequest, TaskOwner, TaskRuntime,
};

#[derive(Clone)]
pub(crate) struct Domain {
    pub runtime: TaskRuntime,
    pub owner: TaskOwner,
    pub calls: Arc<AtomicUsize>,
}
impl ServerHandler for Domain {
    fn supported_protocol_versions(&self) -> std::borrow::Cow<'static, [ProtocolVersion]> {
        std::borrow::Cow::Owned(vec![ProtocolVersion::V_2026_07_28])
    }
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tasks()
                .build(),
        )
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(vec![Tool::new(
            "fixture_task",
            "Create an explicit durable Task fixture",
            serde_json::from_value::<JsonObject>(json!({"type":"object","properties":{}})).unwrap(),
        )]))
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
        assert_eq!(request.name, "fixture_task");
        self.calls.fetch_add(1, Ordering::SeqCst);
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
