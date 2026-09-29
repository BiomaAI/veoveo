//! Native Store and official RMCP qualification for Task-backed resource delivery.
#[path = "../../../testing/fixtures/connection_switch.rs"]
mod connection_switch;
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

use futures::StreamExt;
use rmcp::{
    ClientServiceExt, ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    model::*,
    service::{RequestContext, SubscriptionContext},
};
use serde_json::json;
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use veoveo_task_runtime::{
    CreateTask, DurableTaskService, DurableTaskSubscription, PrincipalKind, RecoveryClass,
    TaskOwner, TaskResourceSubscriptions, TaskRuntime, TaskTransition, subscribe_durable_tasks,
};
use veoveo_types::{
    AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, ResourceAddress, ResourceUri,
    ResourceUriBuilder, ResourceUriParts, TaskId, TaskResourceAddress, TenantId, UriSegment,
    WorkContextId,
};
use veoveo_types::{InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy};

fn owner() -> TaskOwner {
    let principal = PrincipalId::new("resource-observer").unwrap();
    TaskOwner {
        principal_key: principal.to_string(),
        principal_kind: PrincipalKind::User,
        issuer: "https://issuer.example".into(),
        subject: "resource-observer".into(),
        profile: "operator".into(),
        tenant_key: Some("resource-test".into()),
        data_labels: BTreeSet::new(),
        authority: InvocationAuthority {
            work_context: WorkContextId::new("resource-test").unwrap(),
            tenant: TenantId::new("resource-test").unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::new("v1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(principal.clone()),
                initial_grants: vec![],
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Direct {
                initiator: principal,
            },
        },
    }
}

fn draft() -> CreateTask {
    CreateTask {
        task_id: TaskId::new(),
        owner: owner(),
        server: "resource-fixture".into(),
        task_type: const { veoveo_types::TaskTypeName::from_static("produce") },
        request: json!({"value": 1}),
        recovery_class: RecoveryClass::Resume,
        idempotency_key: None,
        ttl_ms: None,
        poll_interval_ms: None,
        retention_pins: BTreeSet::new(),
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Status,
    Output,
}
#[derive(Clone, Copy)]
struct Address {
    id: TaskId,
    kind: Kind,
}
impl ResourceAddress for Address {
    type Error = std::io::Error;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        let invalid = || std::io::Error::other("invalid fixture address");
        let parts = ResourceUriParts::parse(uri.as_str()).map_err(|_| invalid())?;
        let segments = parts.path_segments().collect::<Vec<_>>();
        if parts.scheme() != "fixture"
            || parts.authority() != "task"
            || parts.has_query()
            || segments.len() != 2
        {
            return Err(invalid());
        }
        let id = segments[0].parse().map_err(|_| invalid())?;
        let kind = match segments[1].as_ref() {
            "status" => Kind::Status,
            "output" => Kind::Output,
            _ => return Err(invalid()),
        };
        Ok(Self { id, kind })
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        ResourceUriBuilder::new("fixture://task")
            .unwrap()
            .segment(UriSegment::new(self.id.to_string()).unwrap())
            .segment(
                UriSegment::new(match self.kind {
                    Kind::Status => "status",
                    Kind::Output => "output",
                })
                .unwrap(),
            )
            .build()
            .map_err(std::io::Error::other)
    }
}
impl TaskResourceAddress for Address {
    fn task_id(&self) -> TaskId {
        self.id
    }
}
fn uri(id: TaskId, kind: Kind) -> String {
    Address { id, kind }.to_uri().unwrap().to_string()
}

#[derive(Clone)]
struct Service {
    runtime: TaskRuntime,
    calls: Arc<AtomicUsize>,
}
impl Service {
    fn new(runtime: TaskRuntime) -> Self {
        Self {
            runtime,
            calls: Arc::new(AtomicUsize::new(0)),
        }
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
        unreachable!("fixture only listens")
    }
    async fn get_task(&self, _: &TaskOwner, _: GetTaskParams) -> Result<GetTaskResult, McpError> {
        unreachable!("fixture only listens")
    }
    async fn update_task(&self, _: &TaskOwner, _: UpdateTaskParams) -> Result<(), McpError> {
        unreachable!("fixture only listens")
    }
    async fn cancel_task(&self, _: &TaskOwner, _: String) -> Result<(), McpError> {
        unreachable!("fixture only listens")
    }
    async fn subscribe_tasks(
        &self,
        caller: &TaskOwner,
        ids: Vec<String>,
    ) -> Result<DurableTaskSubscription, McpError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        subscribe_durable_tasks(&self.runtime.for_owner(&caller.clone()), ids).await
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
        Some(filter.clone())
    }
    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        TaskResourceSubscriptions::from_filter::<Address>(context.accepted())?
            .listen(self, context)
            .await
    }
}

#[test]
fn filters_bound_the_combined_task_set_and_reject_other_sources() {
    let ids = (0..257).map(|_| TaskId::new()).collect::<Vec<_>>();
    let filter = SubscriptionFilter::builder()
        .task_ids(ids[..256].iter().map(ToString::to_string))
        .resource_subscriptions([uri(ids[256], Kind::Status)])
        .build();
    assert!(TaskResourceSubscriptions::from_filter::<Address>(&filter).is_err());
    for filter in [
        SubscriptionFilter::builder()
            .resources_list_changed()
            .build(),
        SubscriptionFilter::builder()
            .resource_subscriptions(["fixture://task/not-a-uuid/status"])
            .build(),
        SubscriptionFilter::builder()
            .resource_subscriptions(["fixture://other/a"])
            .build(),
        SubscriptionFilter::builder()
            .resource_subscriptions(vec![uri(ids[0], Kind::Status); 257])
            .build(),
    ] {
        assert!(TaskResourceSubscriptions::from_filter::<Address>(&filter).is_err());
    }
    let filter = SubscriptionFilter::builder()
        .task_ids([ids[0].to_string(), "unknown-handle".into()])
        .resource_subscriptions([
            uri(ids[0], Kind::Status),
            uri(ids[0], Kind::Output),
            uri(ids[0], Kind::Output),
        ])
        .build();
    let selected = TaskResourceSubscriptions::from_filter::<Address>(&filter).unwrap();
    assert_eq!(selected.resource_task_ids().collect::<Vec<_>>(), [ids[0]]);
}

#[tokio::test]
async fn one_owner_watch_multiplexes_resources_and_tasks_and_reconnects() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let service = Service::new(TaskRuntime::new(db.a.clone(), "resource-fixture", "reader"));
        let writer = TaskRuntime::new(db.b.clone(), "resource-fixture", "writer");
        let first = writer.create(draft()).await.unwrap().snapshot.task_id;
        let second = writer.create(draft()).await.unwrap().snapshot.task_id;
        let filter = SubscriptionFilter::builder()
            .task_ids([first.to_string()])
            .resource_subscriptions([
                uri(first, Kind::Status),
                uri(first, Kind::Output),
                uri(second, Kind::Status),
            ])
            .build();
        let mut updates = TaskResourceSubscriptions::from_filter::<Address>(&filter)
            .unwrap()
            .subscribe(&service, &owner())
            .await
            .unwrap();
        let mut baseline_resources = BTreeSet::new();
        for _ in 0..2 {
            let update = updates.next().await.unwrap().unwrap();
            if let Some(task) = update.task {
                assert_eq!(task.task.task_id, first.to_string());
            }
            baseline_resources.extend(update.resources.into_iter().map(String::from));
        }
        assert_eq!(
            baseline_resources,
            filter
                .resource_subscriptions
                .clone()
                .unwrap()
                .into_iter()
                .collect()
        );
        assert_eq!(service.calls.load(Ordering::SeqCst), 1);
        for id in [first, second] {
            writer
                .claim(&id.to_string(), Duration::from_secs(30))
                .await
                .unwrap();
            writer
                .transition(
                    &id.to_string(),
                    TaskTransition::Succeeded {
                        message: "done".into(),
                        result: json!({"value": 42}),
                    },
                )
                .await
                .unwrap();
        }
        let mut completed = false;
        let mut second_changed = false;
        while !completed || !second_changed {
            let update = tokio::time::timeout(Duration::from_secs(3), updates.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            if let Some(task) = update.task {
                assert_eq!(task.task.task_id, first.to_string());
                completed |= task.task.status == TaskStatus::Completed;
                assert_eq!(update.resources.len(), 2);
            } else {
                assert_eq!(
                    update.resources,
                    [Address {
                        id: second,
                        kind: Kind::Status
                    }
                    .to_uri()
                    .unwrap()]
                );
                second_changed = true;
            }
        }
        drop(updates);
        let resource_only = SubscriptionFilter::builder()
            .resource_subscriptions([uri(first, Kind::Output)])
            .build();
        let mut resumed = TaskResourceSubscriptions::from_filter::<Address>(&resource_only)
            .unwrap()
            .subscribe(&service, &owner())
            .await
            .unwrap();
        let update = resumed.next().await.unwrap().unwrap();
        assert!(update.task.is_none());
        assert_eq!(update.resources[0].as_str(), uri(first, Kind::Output));
        assert_eq!(service.calls.load(Ordering::SeqCst), 2);
    })
    .await
    .expect("Task resource delivery exceeded 60 seconds");
}

#[tokio::test]
async fn revoked_resource_updates_are_filtered_in_sql_before_malformed_payload_decoding() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let service = Service::new(TaskRuntime::new(db.a.clone(), "resource-fixture", "reader"));
        let writer = TaskRuntime::new(db.b.clone(), "resource-fixture", "writer");
        let revoked = writer.create(draft()).await.unwrap().snapshot.task_id;
        let sentinel = writer.create(draft()).await.unwrap().snapshot.task_id;
        let filter = SubscriptionFilter::builder().task_ids([sentinel.to_string()])
            .resource_subscriptions([uri(revoked, Kind::Status), uri(sentinel, Kind::Status)]).build();
        let mut updates = TaskResourceSubscriptions::from_filter::<Address>(&filter).unwrap().subscribe(&service, &owner()).await.unwrap();
        for _ in 0..2 { updates.next().await.unwrap().unwrap(); }
        db.b.client().query("UPDATE ONLY $task SET request.owner.data_labels = ['restricted'], request.input = NONE RETURN NONE;
            CREATE outbox_event SET aggregate_type = 'task', aggregate_id = $id, event_type = 'task.fixture', schema_version = 3, payload = {snapshot: {server: 'resource-fixture'}} RETURN NONE;")
            .bind(("task", veoveo_platform_store::task_record_id(revoked))).bind(("id", revoked.to_string())).await.unwrap().check().unwrap();
        assert!(writer.get(&revoked.to_string()).await.is_err());
        writer.claim(&sentinel.to_string(), Duration::from_secs(30)).await.unwrap();
        writer.transition(&sentinel.to_string(), TaskTransition::Succeeded { message: "done".into(), result: json!({"value":42}) }).await.unwrap();
        loop {
            let update = tokio::time::timeout(Duration::from_secs(3), updates.next()).await.unwrap().unwrap().unwrap();
            assert_eq!(update.resources.len(), 1);
            assert_eq!(update.resources[0].as_str(), uri(sentinel, Kind::Status));
            if update.task.unwrap().task.status == TaskStatus::Completed { break; }
        }
        drop(updates);
        let denied = SubscriptionFilter::builder().resource_subscriptions([uri(revoked, Kind::Status)]).build();
        assert!(TaskResourceSubscriptions::from_filter::<Address>(&denied).unwrap().subscribe(&service, &owner()).await.is_err());
        let missing = SubscriptionFilter::builder().resource_subscriptions([uri(TaskId::new(), Kind::Status)]).build();
        assert!(TaskResourceSubscriptions::from_filter::<Address>(&missing).unwrap().subscribe(&service, &owner()).await.is_err());
    }).await.expect("Task resource revocation exceeded 60 seconds");
}

#[tokio::test]
async fn live_source_reconnection_reconciles_current_resources_after_history_loss() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let endpoint = db.a.config().endpoint();
        let switch = connection_switch::ConnectionSwitch::start(endpoint.host_str().unwrap().to_owned(), endpoint.port_or_known_default().unwrap()).await;
        let reader = db.connect_via(&switch.endpoint).await;
        let service = Service::new(TaskRuntime::new(reader, "resource-fixture", "reader"));
        let writer = TaskRuntime::new(db.b.clone(), "resource-fixture", "writer");
        let revoked = writer.create(draft()).await.unwrap().snapshot.task_id;
        let target = writer.create(draft()).await.unwrap().snapshot.task_id;
        let filter = SubscriptionFilter::builder().task_ids([target.to_string()])
            .resource_subscriptions([uri(revoked, Kind::Output), uri(target, Kind::Output)]).build();
        let mut updates = TaskResourceSubscriptions::from_filter::<Address>(&filter).unwrap().subscribe(&service, &owner()).await.unwrap();
        for _ in 0..2 { updates.next().await.unwrap().unwrap(); }
        switch.set_enabled(false).await;
        writer.claim(&target.to_string(), Duration::from_secs(30)).await.unwrap();
        writer.transition(&target.to_string(), TaskTransition::Succeeded { message: "finished during source loss".into(), result: json!({"value":42}) }).await.unwrap();
        db.b.client().query("UPDATE ONLY $task SET request.owner.data_labels = ['restricted'], request.input = NONE RETURN NONE;
            DELETE outbox_event WHERE aggregate_type = 'task' RETURN NONE;")
            .bind(("task", veoveo_platform_store::task_record_id(revoked))).await.unwrap().check().unwrap();
        switch.set_enabled(true).await;
        let update = tokio::time::timeout(Duration::from_secs(15), updates.next()).await.expect("LIVE reconnect must reconcile after event retention loss").unwrap().unwrap();
        assert_eq!(update.task.unwrap().task.status, TaskStatus::Completed);
        assert_eq!(update.resources.len(), 1);
        assert_eq!(update.resources[0].as_str(), uri(target, Kind::Output));
        // Reconciliation follows reconnection, never an idle agent-waking timer.
        assert!(tokio::time::timeout(Duration::from_secs(16), updates.next()).await.is_err());
    }).await.expect("Task resource source recovery exceeded 60 seconds");
}

#[tokio::test]
async fn official_listener_delivers_resource_only_and_mixed_updates_and_cancels() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let server = Service::new(TaskRuntime::new(db.a.clone(), "resource-fixture", "reader"));
        let writer = TaskRuntime::new(db.b.clone(), "resource-fixture", "writer");
        let id = writer.create(draft()).await.unwrap().snapshot.task_id;
        let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn(async move {
            server
                .serve(server_transport)
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let client = ClientConfig::new(
            ClientCapabilities::builder().enable_tasks().build(),
            Implementation::new("task-resource-client", "1.0"),
        )
        .serve_with_lifecycle(
            client_transport,
            rmcp::ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .unwrap();
        for include_task in [false, true] {
            let mut filter = SubscriptionFilter::builder()
                .resource_subscriptions([uri(id, Kind::Output)])
                .build();
            if include_task {
                filter.task_ids = Some(vec![id.to_string()]);
            }
            let mut subscription = client.listen(filter.clone()).await.unwrap();
            assert_eq!(subscription.acknowledged(), &filter);
            let mut resource_seen = false;
            let mut task_seen = false;
            for _ in 0..if include_task { 2 } else { 1 } {
                match subscription.next().await.unwrap().unwrap() {
                    ServerNotification::ResourceUpdatedNotification(update) => {
                        assert_eq!(update.params.uri, uri(id, Kind::Output));
                        resource_seen = true;
                    }
                    ServerNotification::TaskStatusNotification(update) => {
                        assert!(include_task);
                        assert_eq!(update.params.task.task.task_id, id.to_string());
                        task_seen = true;
                    }
                    _ => panic!("unexpected subscription notification"),
                }
            }
            assert!(resource_seen);
            assert_eq!(task_seen, include_task);
            subscription.cancel().await.unwrap();
        }
        client.cancel().await.unwrap();
        tasks.join_next().await.unwrap().unwrap();
    })
    .await
    .expect("official Task resource listener exceeded 60 seconds");
}
