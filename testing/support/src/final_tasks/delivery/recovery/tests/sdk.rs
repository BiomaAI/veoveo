use super::*;
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::*,
    service::{RequestContext, SubscriptionContext},
    transport::streamable_http_server::StreamableHttpService,
};
use std::{
    borrow::Cow,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[derive(Default)]
struct State {
    discoveries: AtomicUsize,
    calls: AtomicUsize,
    listens: AtomicUsize,
    callbacks: AtomicUsize,
    completed: AtomicBool,
    active_listens: AtomicUsize,
    hold_checkpoint: bool,
    hold_completion: bool,
    original_eof: bool,
    end_original: tokio::sync::Notify,
    original_ended: tokio::sync::Notify,
    replacement_entered: tokio::sync::Notify,
    checkpoint_entered: tokio::sync::Notify,
    foreign_after_checkpoint: bool,
    completed_before_checkpoint: bool,
}
#[derive(Clone)]
struct Server(Arc<State>);
struct ActiveListen(Arc<State>);
impl Drop for ActiveListen {
    fn drop(&mut self) {
        self.0.active_listens.fetch_sub(1, Ordering::SeqCst);
    }
}
fn seed() -> Task {
    Task::new(
        "recovery-task",
        TaskStatus::Working,
        "2026-10-10T00:00:00Z",
        "2026-10-10T00:00:00Z",
    )
}
fn detail(completed: bool) -> DetailedTask {
    if !completed {
        return DetailedTask::new(seed(), TaskPayload::Working);
    }
    let Value::Object(result) =
        serde_json::to_value(CallToolResult::structured(serde_json::json!({"answer":7}))).unwrap()
    else {
        unreachable!()
    };
    DetailedTask::new(seed(), TaskPayload::Completed { result })
}
impl ServerHandler for Server {
    async fn discover(&self, _: RequestContext<RoleServer>) -> Result<DiscoverResult, ErrorData> {
        self.0.discoveries.fetch_add(1, Ordering::SeqCst);
        Ok(DiscoverResult::from_server_info(
            self.supported_protocol_versions().into_owned(),
            self.get_info(),
        ))
    }
    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(&[ProtocolVersion::V_2026_07_28])
    }
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tasks()
                .build(),
        )
    }
    async fn call_tool(
        &self,
        _: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        self.0.calls.fetch_add(1, Ordering::SeqCst);
        Ok(CallToolResponse::Task(CreateTaskResult::new(seed())))
    }
    async fn get_task(
        &self,
        request: GetTaskParams,
        _: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, ErrorData> {
        assert_eq!(request.task_id, "recovery-task");
        let mut task = detail(self.0.completed.load(Ordering::SeqCst));
        if self.0.foreign_after_checkpoint && self.0.callbacks.load(Ordering::SeqCst) > 0 {
            task.task.created_at = "2026-10-11T00:00:00Z".into();
        }
        Ok(GetTaskResult::new(task))
    }
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        Some(requested.clone())
    }
    async fn listen(&self, context: SubscriptionContext) -> Result<(), ErrorData> {
        self.0.active_listens.fetch_add(1, Ordering::SeqCst);
        let _active = ActiveListen(self.0.clone());
        let index = self.0.listens.fetch_add(1, Ordering::SeqCst);
        let completed =
            (index > 0 && !self.0.hold_completion) || self.0.completed_before_checkpoint;
        if completed {
            self.0.completed.store(true, Ordering::SeqCst);
        }
        context
            .sink()
            .notify_task_status(detail(completed))
            .await
            .map_err(|_| ErrorData::internal_error("native notification failed", None))?;
        if index > 0 {
            self.0.replacement_entered.notify_one();
        }
        if index == 0 {
            tokio::select! {()=context.cancelled()=>{},()=self.0.end_original.notified()=>{}}
            self.0.original_ended.notify_one();
        } else {
            context.cancelled().await;
        }
        Ok(())
    }
}
struct Checkpoint(Arc<State>);
impl WorkingTaskRecovery for Checkpoint {
    fn replacement_working(
        &mut self,
        id: &CanonicalTaskId,
        created: &Task,
        current: &DetailedTask,
    ) -> Result<()> {
        working(created, current, id)
    }
    fn checkpoint<'a>(
        &'a mut self,
        context: WorkingCheckpoint<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            assert_eq!(context.task_id().as_str(), "recovery-task");
            assert_eq!(
                context.created_task().created_at,
                context.current_working().task.created_at
            );
            assert!(context.deadline() > tokio::time::Instant::now());
            self.0.callbacks.fetch_add(1, Ordering::SeqCst);
            self.0.checkpoint_entered.notify_one();
            if self.0.original_eof {
                self.0.end_original.notify_one();
                self.0.original_ended.notified().await;
            }
            if self.0.hold_checkpoint {
                std::future::pending::<()>().await;
            }
            Ok(())
        })
    }
}
struct OwnedServer(Option<tokio::task::JoinHandle<std::io::Result<()>>>);
impl Drop for OwnedServer {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}
impl OwnedServer {
    async fn stop(mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

#[tokio::test]
async fn recovery_sdk_reconnects_once_without_redispatch_and_rejects_identity_or_completed_shortcuts()
-> Result<()> {
    const CHILD: &str = "VEOVEO_NATIVE_RECOVERY_CONTROL";
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in [
            "success",
            "foreign",
            "early",
            "timeout",
            "cancel",
            "cancel_checkpoint",
            "eof",
        ] {
            let root = tempfile::tempdir()?;
            let mut command = tokio::process::Command::new(std::env::current_exe()?);
            command.arg("final_tasks::delivery::recovery::tests::sdk::recovery_sdk_reconnects_once_without_redispatch_and_rejects_identity_or_completed_shortcuts").arg("--exact").arg("--nocapture").env(CHILD,mode).env("VEOVEO_SMOKE_LOCAL_GROUPS",root.path()).kill_on_drop(true);
            let output = tokio::time::timeout(Duration::from_secs(20), command.output()).await??;
            ensure!(
                output.status.success(),
                "isolated recovery SDK mode {mode} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return Ok(());
    };
    let _ = rustls::crypto::ring::default_provider().install_default();
    {
        let foreign = mode == "foreign";
        let early = mode == "early";
        let interrupted = mode == "timeout" || mode == "cancel" || mode == "cancel_checkpoint";
        let state = Arc::new(State {
            foreign_after_checkpoint: foreign,
            completed_before_checkpoint: early,
            hold_checkpoint: mode == "cancel_checkpoint",
            hold_completion: mode == "timeout" || mode == "cancel",
            original_eof: mode == "eof",
            ..State::default()
        });
        let factory = state.clone();
        let service = StreamableHttpService::new(
            move || Ok(Server(factory.clone())),
            veoveo_mcp_contract::stateless_session_manager(),
            veoveo_mcp_contract::canonical_streamable_http_server_config(),
        );
        let router = axum::Router::new().nest_service("/mcp", service);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let endpoint = format!("http://{}/mcp", listener.local_addr()?);
        let server = OwnedServer(Some(tokio::spawn(async move {
            axum::serve(listener, router).await
        })));
        let mut checkpoint = Checkpoint(state.clone());
        let created = Arc::new(AtomicUsize::new(0));
        let observed = created.clone();
        let result = tokio::time::timeout(
            Duration::from_secs(15),
            owner::run(async {
                let client=FinalTaskSmokeClient::new(&endpoint, "native-private-token".into());
                let operation=client
                    .run_tool_delivered_recovered(
                        "native-task",
                        serde_json::json!({}),
                        Duration::from_secs(3),
                        move |id, task| {
                            assert_eq!(id.as_str(), task.task_id);
                            observed.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        },
                        &mut checkpoint,
                    )
                    ;
                tokio::pin!(operation);
                if mode=="cancel" || mode=="cancel_checkpoint" {
                    tokio::select! {result=&mut operation=>result,_=async {if mode=="cancel" {state.replacement_entered.notified().await;}else{state.checkpoint_entered.notified().await;}}=>bail!("native sibling cancellation")}
                }else{operation.await}
            }),
        )
        .await;
        let result = result?;
        // Observe SDK-owned subscription shutdown before terminating the test issuer.
        let drained = tokio::time::timeout(Duration::from_secs(2), async {
            while state.active_listens.load(Ordering::SeqCst) != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await;
        server.stop().await;
        drained?;
        if foreign || early || interrupted {
            assert!(result.is_err());
        } else {
            assert_eq!(
                result?.result.structured_content,
                Some(serde_json::json!({"answer":7}))
            );
        }
        assert_eq!(state.calls.load(Ordering::SeqCst), 1);
        if !foreign && !early && !interrupted {
            assert_eq!(
                state.discoveries.load(Ordering::SeqCst),
                2,
                "successful recovery must settle the original Task cleanup without a third connection"
            );
        }
        assert_eq!(created.load(Ordering::SeqCst), 1);
        assert_eq!(state.callbacks.load(Ordering::SeqCst), usize::from(!early));
        assert_eq!(
            state.listens.load(Ordering::SeqCst),
            if foreign || early || mode == "cancel_checkpoint" {
                1
            } else {
                2
            }
        );
    }
    Ok(())
}
