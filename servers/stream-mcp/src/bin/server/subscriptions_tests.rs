//! Native subscription behavior over isolated Store and inert live-session metadata.
use rmcp::{
    ClientServiceExt, RoleServer, ServerHandler, ServiceExt, model::*, service::RequestContext,
};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use veoveo_stream_mcp::uris;
use veoveo_task_runtime::{DurableTaskSubscription, TaskStatus as StoredTaskStatus};
use veoveo_types::TaskId;

use super::*;
use crate::{
    live::subscription_fixture,
    store_fixture as fixture, task_results,
    test_support::{create, current_output, finish, owner},
};

#[path = "../../../../../testing/fixtures/connection_switch.rs"]
mod connection_switch;

#[derive(Clone)]
struct Service {
    runtime: TaskRuntime,
    live: Arc<LiveSessionManager>,
    calls: Arc<AtomicUsize>,
}
impl Service {
    fn new(runtime: TaskRuntime, live: Arc<LiveSessionManager>) -> Self {
        Self {
            runtime,
            live,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
    async fn subscribe(
        &self,
        filter: &SubscriptionFilter,
        caller: TaskOwner,
    ) -> Result<Updates, McpError> {
        Selection::new(filter)?
            .subscribe(
                self,
                &caller,
                &self.runtime,
                self.live.clone(),
                caller.clone(),
            )
            .await
    }
}
impl DurableTaskService for Service {
    type Caller = TaskOwner;
    fn authenticate(&self, _: &RequestContext<RoleServer>) -> Result<TaskOwner, McpError> {
        Ok(owner())
    }
    async fn start_tool_task(
        &self,
        _: &TaskOwner,
        _: CallToolRequestParams,
    ) -> Result<Option<CreateTaskResult>, McpError> {
        unreachable!("subscription fixture")
    }
    async fn get_task(
        &self,
        caller: &TaskOwner,
        request: GetTaskParams,
    ) -> Result<GetTaskResult, McpError> {
        task_results::get_task(&self.runtime, caller, request).await
    }
    async fn update_task(&self, _: &TaskOwner, _: UpdateTaskParams) -> Result<(), McpError> {
        unreachable!("subscription fixture")
    }
    async fn cancel_task(&self, _: &TaskOwner, _: String) -> Result<(), McpError> {
        unreachable!("subscription fixture")
    }
    async fn subscribe_tasks(
        &self,
        caller: &TaskOwner,
        ids: Vec<String>,
    ) -> Result<DurableTaskSubscription, McpError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        task_results::subscribe_tasks(&self.runtime, caller.clone(), ids).await
    }
}
impl ServerHandler for Service {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_resources()
                .enable_resources_subscribe()
                .enable_tasks()
                .build(),
        )
    }
    fn accepted_subscription_filter(
        &self,
        filter: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        crate::resources::accepted_subscription_filter(filter)
    }
    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        listen(
            self,
            &owner(),
            &self.runtime,
            self.live.clone(),
            owner(),
            context,
        )
        .await
    }
}

fn run(id: TaskId) -> RunId {
    RunId::try_from(id).unwrap()
}
async fn complete(writer: &TaskRuntime, id: TaskId) {
    let result =
        task_results::recording_result(serde_json::from_value(current_output(id)).unwrap())
            .unwrap();
    finish(writer, id, serde_json::to_value(result).unwrap()).await;
}
async fn next(updates: &mut Updates) -> TaskResourceUpdate {
    tokio::time::timeout(Duration::from_secs(10), updates.next())
        .await
        .expect("subscription update timeout")
        .unwrap()
        .unwrap()
}

#[test]
fn selections_bound_the_whole_request_and_keep_the_sources_distinct() {
    let id = TaskId::new();
    let session: SessionId = "01983da0-0000-7000-8000-000000000001".parse().unwrap();
    let filter = SubscriptionFilter::builder()
        .task_ids([id.to_string()])
        .resource_subscriptions([
            uris::run_uri(run(id)).to_string(),
            uris::results_uri(run(id)).to_string(),
            uris::session_uri(session).to_string(),
            uris::session_results_uri(session).to_string(),
        ])
        .build();
    let selected = Selection::new(&filter).unwrap();
    assert_eq!(selected.runs.resource_task_ids().collect::<Vec<_>>(), [id]);
    assert_eq!(selected.sessions.len(), 1);
    assert_eq!(selected.sessions[&session].len(), 2);
    for filter in [
        SubscriptionFilter::builder()
            .resources_list_changed()
            .build(),
        SubscriptionFilter::builder()
            .resource_subscriptions(["stream://runs"])
            .build(),
        SubscriptionFilter::builder()
            .resource_subscriptions(std::iter::repeat_n(
                uris::session_uri(session).to_string(),
                257,
            ))
            .build(),
        SubscriptionFilter::builder()
            .task_ids((0..256).map(|_| TaskId::new().to_string()))
            .resource_subscriptions([uris::run_uri(run(id)).to_string()])
            .build(),
    ] {
        assert!(Selection::new(&filter).is_err());
    }
}

#[tokio::test]
async fn mixed_sources_deliver_cross_instance_runs_live_updates_and_reconnect_baselines() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "stream", "writer");
        let (live, session, hub) = subscription_fixture::new(owner()).await;
        let service = Service::new(TaskRuntime::new(db.b.clone(), "stream", "reader"), live);
        let first = TaskId::new();
        let second = TaskId::new();
        for id in [first, second] {
            create(&writer, owner(), id).await;
        }
        let live_uris = [
            uris::session_uri(session).to_uri(),
            uris::session_results_uri(session).to_uri(),
            uris::session_preview_uri(session).to_uri(),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        let filter = SubscriptionFilter::builder()
            .task_ids([first.to_string()])
            .resource_subscriptions(
                [
                    uris::run_uri(run(first)).to_string(),
                    uris::results_uri(run(first)).to_string(),
                    uris::results_uri(run(second)).to_string(),
                ]
                .into_iter()
                .chain(live_uris.iter().map(ToString::to_string)),
            )
            .build();
        let mut updates = service.subscribe(&filter, owner()).await.unwrap();
        for _ in 0..3 {
            next(&mut updates).await;
        }
        assert_eq!(service.calls.load(Ordering::SeqCst), 1);

        complete(&writer, first).await;
        complete(&writer, second).await;
        let mut completed = false;
        let mut other_changed = false;
        while !completed || !other_changed {
            let update = next(&mut updates).await;
            if let Some(task) = update.task {
                assert_eq!(task.task.task_id, first.to_string());
                completed |= matches!(task.payload, TaskPayload::Completed { .. });
                assert_eq!(update.resources.len(), 2);
            } else {
                assert_eq!(update.resources, [uris::results_uri(run(second)).to_uri()]);
                other_changed = true;
            }
        }
        assert_eq!(
            writer
                .get(&second.to_string())
                .await
                .unwrap()
                .unwrap()
                .status,
            StoredTaskStatus::Succeeded
        );
        // Local run signals and unrequested resources cannot supply run invalidations.
        hub.notify_resource_updated(uris::run_uri(run(first))).await;
        hub.notify_resource_updated("stream://pipelines").await;
        hub.notify_resource_updated(uris::session_preview_uri(session))
            .await;
        let update = next(&mut updates).await;
        assert!(update.task.is_none());
        assert_eq!(
            update.resources,
            [uris::session_preview_uri(session).to_uri()]
        );

        // Overflow reconciles only live addresses; durable runs have their own source.
        for _ in 0..300 {
            hub.notify_resource_updated(uris::session_preview_uri(session))
                .await;
        }
        let update = next(&mut updates).await;
        assert!(update.task.is_none());
        assert_eq!(
            update.resources.into_iter().collect::<BTreeSet<_>>(),
            live_uris
        );
        drop(updates);
        let mut reconnected = service.subscribe(&filter, owner()).await.unwrap();
        let mut seen = BTreeSet::new();
        for _ in 0..3 {
            let update = next(&mut reconnected).await;
            if let Some(task) = update.task {
                assert!(matches!(task.payload, TaskPayload::Completed { .. }));
            }
            seen.extend(update.resources.into_iter().map(String::from));
        }
        assert_eq!(
            seen,
            filter.resource_subscriptions.unwrap().into_iter().collect()
        );
        assert_eq!(service.calls.load(Ordering::SeqCst), 2);
    })
    .await
    .expect("mixed Stream delivery exceeded 90 seconds");
}

#[tokio::test]
async fn denied_resources_and_lost_live_sessions_never_emit_updates() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "stream", "reader");
        let (live, session, hub) = subscription_fixture::new(owner()).await;
        let service = Service::new(runtime.clone(), live.clone());
        let id = TaskId::new();
        create(&runtime, owner(), id).await;
        db.b.client().query("UPDATE ONLY $task SET request.owner.data_labels = ['restricted'], request.input = NONE RETURN NONE;")
            .bind(("task", veoveo_platform_store::task_record_id(id))).await.unwrap().check().unwrap();
        assert!(runtime.get(&id.to_string()).await.is_err());
        let filter = SubscriptionFilter::builder().resource_subscriptions([uris::run_uri(run(id)).to_string()]).build();
        assert_eq!(service.subscribe(&filter, owner()).await.err().unwrap().message, "Stream run not found");
        let filter = SubscriptionFilter::builder().resource_subscriptions([uris::session_uri(session).to_string()]).build();
        let mut denied = owner();
        denied.authority.work_context = veoveo_types::WorkContextId::new("other-context").unwrap();
        assert!(service.subscribe(&filter, denied).await.is_err());
        let mut updates = service.subscribe(&filter, owner()).await.unwrap();
        next(&mut updates).await;
        subscription_fixture::remove(&live, session).await;
        hub.notify_resource_updated(uris::session_uri(session)).await;
        assert_eq!(updates.next().await.unwrap().err().unwrap().message, "Stream session not found");
        assert!(service.subscribe(&filter, owner()).await.is_err());
    }).await.expect("Stream subscription isolation exceeded 60 seconds");
}

#[tokio::test]
async fn store_reconnect_reconciles_runs_without_interrupting_live_updates() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let endpoint = db.a.config().endpoint();
        let switch = connection_switch::ConnectionSwitch::start(
            endpoint.host_str().unwrap().to_owned(),
            endpoint.port_or_known_default().unwrap(),
        )
        .await;
        let reader = db.connect_via(&switch.endpoint).await;
        let writer = TaskRuntime::new(db.a.clone(), "stream", "writer");
        let (live, session, hub) = subscription_fixture::new(owner()).await;
        let service = Service::new(TaskRuntime::new(reader, "stream", "reader"), live);
        let id = TaskId::new();
        create(&writer, owner(), id).await;
        let filter = SubscriptionFilter::builder()
            .task_ids([id.to_string()])
            .resource_subscriptions([
                uris::results_uri(run(id)).to_string(),
                uris::session_preview_uri(session).to_string(),
            ])
            .build();
        let mut updates = service.subscribe(&filter, owner()).await.unwrap();
        for _ in 0..2 {
            next(&mut updates).await;
        }
        switch.set_enabled(false).await;
        complete(&writer, id).await;
        hub.notify_resource_updated(uris::session_preview_uri(session))
            .await;
        let update = next(&mut updates).await;
        assert!(update.task.is_none());
        assert_eq!(
            update.resources,
            [uris::session_preview_uri(session).to_uri()]
        );
        switch.set_enabled(true).await;
        let update = next(&mut updates).await;
        assert!(matches!(
            update.task.unwrap().payload,
            TaskPayload::Completed { .. }
        ));
        assert_eq!(update.resources, [uris::results_uri(run(id)).to_uri()]);
        assert_eq!(service.calls.load(Ordering::SeqCst), 1);
    })
    .await
    .expect("Stream source reconnect exceeded 90 seconds");
}

#[tokio::test]
async fn official_listener_acknowledges_mixed_filters_and_cancels() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "stream", "listener");
        let (live, session, _) = subscription_fixture::new(owner()).await;
        let service = Service::new(runtime.clone(), live);
        let id = TaskId::new();
        create(&runtime, owner(), id).await;
        let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn(async move {
            service
                .serve(server_transport)
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let client = ClientConfig::new(
            ClientCapabilities::builder().enable_tasks().build(),
            Implementation::new("stream-subscription-client", "1.0"),
        )
        .serve_with_lifecycle(
            client_transport,
            rmcp::ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .unwrap();
        let filter = SubscriptionFilter::builder()
            .task_ids([id.to_string()])
            .resource_subscriptions([
                uris::results_uri(run(id)).to_string(),
                uris::session_preview_uri(session).to_string(),
            ])
            .build();
        for _ in 0..2 {
            let mut subscription = client.listen(filter.clone()).await.unwrap();
            assert_eq!(subscription.acknowledged(), &filter);
            let mut resources = BTreeSet::new();
            let mut task_seen = false;
            for _ in 0..3 {
                match subscription.next().await.unwrap().unwrap() {
                    ServerNotification::ResourceUpdatedNotification(update) => {
                        resources.insert(update.params.uri);
                    }
                    ServerNotification::TaskStatusNotification(update) => {
                        assert_eq!(update.params.task.task.task_id, id.to_string());
                        task_seen = true;
                    }
                    _ => panic!("unexpected notification"),
                }
            }
            assert_eq!(
                resources,
                filter
                    .resource_subscriptions
                    .clone()
                    .unwrap()
                    .into_iter()
                    .collect()
            );
            assert!(task_seen);
            subscription.cancel().await.unwrap();
        }
        client.cancel().await.unwrap();
        tasks.join_next().await.unwrap().unwrap();
    })
    .await
    .expect("Stream official listener exceeded 60 seconds");
}
