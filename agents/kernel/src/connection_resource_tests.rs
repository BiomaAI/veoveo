//! Actual SDK listener admission does not imply a current-resource read.
use super::*;
use rig::tool::server::ToolServer;
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::*,
    service::{RequestContext, SubscriptionContext},
};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Default)]
struct CurrentResource {
    version: Arc<AtomicU64>,
    reads: Arc<AtomicU64>,
}
impl ServerHandler for CurrentResource {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_resources_subscribe()
                .build(),
        )
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(Vec::new())
            .with_ttl_ms(0)
            .with_cache_scope(CacheScope::Private))
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            self.version.load(Ordering::SeqCst).to_string(),
            request.uri,
        )])
        .into())
    }
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        Some(requested.supported_by(&self.get_info().capabilities))
    }
    async fn listen(&self, context: SubscriptionContext) -> Result<(), ErrorData> {
        context.cancelled().await;
        Ok(())
    }
}
#[derive(Clone, Default)]
struct Invalidations(Arc<AtomicU64>);
impl McpResourceNotificationHandler for Invalidations {
    fn on_resource_updated(&self, _: ResourceUpdatedNotificationParam) -> WasmBoxedFuture<'_, ()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(std::future::ready(()))
    }
}

#[tokio::test]
async fn sdk_replacement_ack_does_not_reconcile_missed_resource_updates() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let state = CurrentResource::default();
        let observations = Invalidations::default();
        for generation in 0..2 {
            if generation == 1 {
                state.version.store(7, Ordering::SeqCst);
            }
            let server = state.clone();
            let (client_to_server, server_from_client) = tokio::io::duplex(8192);
            let (server_to_client, client_from_server) = tokio::io::duplex(8192);
            let server_task = tokio::spawn(async move {
                let running = server
                    .serve((server_from_client, server_to_client))
                    .await
                    .unwrap();
                let _ = running.waiting().await;
            });
            let guard = McpClientHandler::new(
                McpClientConfig::new(Implementation::new("resource-recovery-control", "0.1.0")),
                ToolServer::new().run(),
            )
            .with_resource_subscriptions(["memo://current"], observations.clone())
            .connect((client_from_server, client_to_server))
            .await
            .unwrap();
            assert_eq!(
                guard
                    .acknowledged_subscription_filter()
                    .unwrap()
                    .resource_subscriptions
                    .as_ref()
                    .unwrap(),
                &["memo://current"]
            );
            assert_eq!(
                state.reads.load(Ordering::SeqCst),
                0,
                "ACK does not perform a current read"
            );
            assert_eq!(
                observations.0.load(Ordering::SeqCst),
                0,
                "offline change is not replayed as a resource update"
            );
            if generation == 1 {
                let read = guard
                    .request_handle()
                    .read_resource(ReadResourceRequestParams::new("memo://current"))
                    .await
                    .unwrap();
                let ReadResourceResponse::Complete(read) = read else {
                    panic!("expected finite current resource");
                };
                let ResourceContents::TextResourceContents { text, .. } = &read.contents[0] else {
                    panic!("expected current text");
                };
                assert_eq!(
                    text, "7",
                    "authoritative read still exposes the missed change"
                );
            }
            guard.cancel().await.unwrap();
            server_task.await.unwrap();
        }
    })
    .await
    .expect("resource recovery control deadline");
}

/// A real hosted resource listener, with one deliberately interrupted request.
/// The peer remains alive: Rig must re-listen rather than rotate its connection.
#[derive(Clone, Default)]
struct ReconciledResource {
    current: CurrentResource,
    hub: Arc<veoveo_mcp_contract::SubscriptionHub>,
    listeners: Arc<AtomicU64>,
    interrupt: Arc<tokio::sync::Notify>,
}
impl ServerHandler for ReconciledResource {
    fn get_info(&self) -> ServerConfig {
        self.current.get_info()
    }
    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        self.current.list_tools(request, context).await
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        self.current.read_resource(request, context).await
    }
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        self.current.accepted_subscription_filter(requested)
    }
    async fn listen(&self, context: SubscriptionContext) -> Result<(), ErrorData> {
        let generation = self.listeners.fetch_add(1, Ordering::SeqCst);
        tokio::select! {
            result = veoveo_mcp_contract::listen_resources(context, &self.hub, None) => result,
            () = self.interrupt.notified(), if generation == 0 => {
                Err(ErrorData::internal_error("fixture listener interrupted", None))
            }
        }
    }
}

#[derive(Clone, Default)]
struct ReconciliationWakes {
    wakes: Arc<Mutex<Vec<veoveo_agent_runtime::NewWake>>>,
    changed: Arc<tokio::sync::Notify>,
}
impl McpResourceNotificationHandler for ReconciliationWakes {
    fn on_resource_updated(
        &self,
        params: ResourceUpdatedNotificationParam,
    ) -> WasmBoxedFuture<'_, ()> {
        Box::pin(async move {
            let declared = BTreeSet::from(["memo://current".to_owned()]);
            let wake = resource_update_wake(&declared, &params).expect("admitted exact resource");
            self.wakes.lock().await.push(wake);
            self.changed.notify_one();
        })
    }
}
impl ReconciliationWakes {
    async fn wait_for(&self, count: usize) {
        loop {
            let changed = self.changed.notified();
            if self.wakes.lock().await.len() >= count {
                return;
            }
            changed.await;
        }
    }
}

#[tokio::test]
async fn shared_listener_reconciles_after_actual_sdk_automatic_relisten() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let state = ReconciledResource::default();
        let server = state.clone();
        let observations = ReconciliationWakes::default();
        let (client_to_server, server_from_client) = tokio::io::duplex(8192);
        let (server_to_client, client_from_server) = tokio::io::duplex(8192);
        let server_task = tokio::spawn(async move {
            let running = server.serve((server_from_client, server_to_client)).await.unwrap();
            let _ = running.waiting().await;
        });
        let guard = McpClientHandler::new(McpClientConfig::new(Implementation::new("hosted-resource-recovery-control", "0.1.0")), ToolServer::new().run())
            .with_resource_subscriptions(["memo://current"], observations.clone())
            .connect((client_from_server, client_to_server)).await.unwrap();
        observations.wait_for(1).await;
        assert_eq!(state.listeners.load(Ordering::SeqCst), 1);
        // No hub update is published for this change. Only a new establishment
        // can tell the consumer to reconcile the current resource.
        state.current.version.store(7, Ordering::SeqCst);
        state.interrupt.notify_one();
        observations.wait_for(2).await;
        assert_eq!(state.listeners.load(Ordering::SeqCst), 2, "same peer automatically re-listened");
        assert_eq!(state.current.reads.load(Ordering::SeqCst), 0, "invalidation does not claim current state");
        let ReadResourceResponse::Complete(current) = guard.request_handle().read_resource(ReadResourceRequestParams::new("memo://current")).await.unwrap() else { panic!("finite current resource required"); };
        let ResourceContents::TextResourceContents { text, .. } = &current.contents[0] else { panic!("text resource required"); };
        assert_eq!(text, "7");
        let wakes = observations.wakes.lock().await;
        assert_eq!(wakes.len(), 2);
        assert!(wakes.iter().all(|wake| matches!(&wake.payload, veoveo_agent_runtime::persistence::WakePayload::ResourceChanged { uri } if uri.as_str() == "memo://current")));
        assert_ne!(wakes[0].wake_id, wakes[1].wake_id, "invalidation is not an exactly-once semantic effect");
        drop(wakes);
        guard.cancel().await.unwrap();
        server_task.await.unwrap();
    }).await.expect("automatic resource re-listen deadline");
}
