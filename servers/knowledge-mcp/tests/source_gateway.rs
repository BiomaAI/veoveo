use rmcp::{
    ClientServiceExt, ServerHandler, ServiceExt,
    model::*,
    service::{RequestContext, RoleServer, SubscriptionContext},
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use veoveo_knowledge_contract::{CollectionApproval, KnowledgeCollectionApproval};
use veoveo_knowledge_mcp::source::{
    ApprovedCollection, DiscoveryScope, GatewaySource, ObservableSource, SourceListener,
};
use veoveo_mcp_contract::{
    GatewayDiscoveryDegradation, GatewayDiscoveryFailure, GatewayDiscoveryFailureCode,
    GatewayDiscoverySurface, SubscriptionHub,
};
use veoveo_mcp_knowledge_extension::{self as knowledge, *};
use veoveo_types::*;

#[derive(Clone, Copy)]
enum Mode {
    PendingOnce,
    Complete,
    Duplicate,
    Missing,
    Unavailable,
    ForeignContract,
}
struct State {
    mode: Mutex<Mode>,
    hub: SubscriptionHub,
    root_ready: Notify,
    root_requested: Notify,
    pages: AtomicUsize,
    contracts: AtomicUsize,
}
#[derive(Clone)]
struct Gateway(Arc<State>);
fn descriptor() -> CollectionDescriptor {
    CollectionDescriptor::new(
        "fixture.records".parse().unwrap(),
        "record".parse().unwrap(),
        ResourceTemplateUri::new("fixture://records{?cursor}").unwrap(),
        Freshness::max_age(30),
        ChangeSignal::Listen,
        AccessModel::Profile,
        IndexingMode::Content,
    )
    .unwrap()
}
fn scope() -> DiscoveryScope {
    DiscoveryScope {
        tenant: "test".parse().unwrap(),
        control_revision: Sha256Digest::from_bytes([1; 32]),
        collections: BTreeMap::from([(
            "fixture.records".parse().unwrap(),
            ApprovedCollection {
                scheme: "fixture".parse().unwrap(),
                approval: KnowledgeCollectionApproval {
                    collection: "fixture.records".parse().unwrap(),
                    mode: CollectionApproval::Index,
                    stewards: ["reviewers".parse().unwrap()].into(),
                    authoritative_for: Default::default(),
                    data_labels: Default::default(),
                },
            },
        )]),
    }
}
impl ServerHandler for Gateway {
    fn get_info(&self) -> ServerConfig {
        let mut capabilities = ServerCapabilities::builder()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .build();
        knowledge::server::declare(&mut capabilities);
        ServerConfig::new(capabilities)
    }
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        Some(requested.clone())
    }
    async fn listen(&self, context: SubscriptionContext) -> Result<(), ErrorData> {
        if context.accepted().resource_subscriptions.is_some() {
            self.0.root_requested.notify_one();
            tokio::select! { _ = context.cancelled() => return Ok(()), _ = self.0.root_ready.notified() => () }
        }
        veoveo_mcp_contract::listen_resources(context, &self.0.hub, None).await
    }
    async fn list_resource_templates(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        let count = self.0.pages.fetch_add(1, Ordering::SeqCst);
        let mode = *self.0.mode.lock().unwrap();
        let pending = matches!(mode, Mode::PendingOnce) && count == 0;
        let unavailable = matches!(mode, Mode::Unavailable);
        let mut template = ResourceTemplate::new("fixture://member/{id}", "Records");
        knowledge::server::attach_collection(&mut template, &descriptor());
        let templates = if pending || unavailable || matches!(mode, Mode::Missing) {
            vec![]
        } else if matches!(mode, Mode::Duplicate) {
            vec![template.clone(), template]
        } else {
            vec![template]
        };
        let meta = if pending || unavailable {
            GatewayDiscoveryDegradation::new([GatewayDiscoveryFailure {
                server: "fixture".parse().unwrap(),
                surface: GatewayDiscoverySurface::ResourceTemplates,
                code: if pending {
                    GatewayDiscoveryFailureCode::DiscoveryPending
                } else {
                    GatewayDiscoveryFailureCode::UpstreamUnavailable
                },
            }])
            .into_meta()
        } else {
            None
        };
        if pending {
            self.0.hub.notify_resource_list_changed().await;
        }
        Ok(ListResourceTemplatesResult {
            resource_templates: templates,
            next_cursor: None,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(60_000),
            cache_scope: Some(CacheScope::Private),
            meta,
        })
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        assert_eq!(request.uri, "fixture://contract");
        let intent: IndexingReadIntent =
            serde_json::from_value(context.meta.get(INDEXING_READ_KEY).unwrap().clone()).unwrap();
        assert_eq!(intent.collection, *descriptor().collection());
        assert_eq!(intent.kind, IndexingReadKind::SourceContract);
        assert!(knowledge::server::requested(Some(&context.meta)).unwrap());
        self.0.contracts.fetch_add(1, Ordering::SeqCst);
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration {
            server: if matches!(*self.0.mode.lock().unwrap(), Mode::ForeignContract) {
                "foreign"
            } else {
                "fixture"
            }
            .into(),
            contract_revision: 3,
            compliance: vec![],
        };
        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            serde_json::to_string(&declaration).unwrap(),
            request.uri,
        )])
        .into())
    }
}

#[tokio::test]
async fn discovery_waits_for_native_completion_and_subscription_waits_for_source_readiness() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let state = Arc::new(State {
            mode: Mutex::new(Mode::PendingOnce),
            hub: SubscriptionHub::new(),
            root_ready: Notify::new(),
            root_requested: Notify::new(),
            pages: AtomicUsize::new(0),
            contracts: AtomicUsize::new(0),
        });
        let (server_io, client_io) = tokio::io::duplex(64 * 1024);
        let handler = Gateway(state.clone());
        let server = tokio::spawn(async move { handler.serve(server_io).await.unwrap() });
        let client = ()
            .serve_with_lifecycle(
                client_io,
                rmcp::ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            )
            .await
            .unwrap();
        let server = server.await.unwrap();
        let source = GatewaySource::from_authenticated_peer(client.peer().clone());
        let mut discovered = source.discover(&scope()).await.unwrap();
        assert_eq!(discovered.registrations.len(), 1);
        assert!(
            state.pages.load(Ordering::SeqCst) >= 2,
            "partial catalogs must never finish discovery, even with a cache TTL"
        );
        assert_eq!(state.contracts.load(Ordering::SeqCst), 1);
        let descriptor = descriptor();
        let opening = source.listen(&descriptor);
        tokio::pin!(opening);
        assert!(
            tokio::time::timeout(Duration::from_millis(25), &mut opening)
                .await
                .is_err()
        );
        state.root_requested.notified().await;
        state.root_ready.notify_one();
        let mut listener = opening.await.unwrap();
        state.hub.notify_resource_updated("fixture://records").await;
        listener.changed().await.unwrap();
        state.hub.notify_resource_list_changed().await;
        discovered.listener.changed().await.unwrap();
        server.cancel().await.unwrap();
        assert!(listener.changed().await.is_err());
        client.cancel().await.unwrap();
    })
    .await
    .expect("source discovery exceeded 30 seconds");
}

#[tokio::test]
async fn incomplete_ambiguous_and_foreign_source_catalogs_fail_closed() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let state = Arc::new(State {
            mode: Mutex::new(Mode::Complete),
            hub: SubscriptionHub::new(),
            root_ready: Notify::new(),
            root_requested: Notify::new(),
            pages: AtomicUsize::new(0),
            contracts: AtomicUsize::new(0),
        });
        let (server_io, client_io) = tokio::io::duplex(64 * 1024);
        let handler = Gateway(state.clone());
        let server = tokio::spawn(async move { handler.serve(server_io).await.unwrap() });
        let client = ()
            .serve_with_lifecycle(
                client_io,
                rmcp::ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            )
            .await
            .unwrap();
        let server = server.await.unwrap();
        let source = GatewaySource::from_authenticated_peer(client.peer().clone());
        for mode in [
            Mode::Duplicate,
            Mode::Missing,
            Mode::Unavailable,
            Mode::ForeignContract,
        ] {
            *state.mode.lock().unwrap() = mode;
            assert!(source.discover(&scope()).await.is_err());
        }
        *state.mode.lock().unwrap() = Mode::Complete;
        let mut wrong_scheme = scope();
        wrong_scheme.collections.values_mut().next().unwrap().scheme = "foreign".parse().unwrap();
        assert!(source.discover(&wrong_scheme).await.is_err());
        assert_eq!(
            source.discover(&scope()).await.unwrap().registrations[0].descriptor,
            descriptor()
        );
        client.cancel().await.unwrap();
        server.cancel().await.unwrap();
    })
    .await
    .expect("source discovery refusal exceeded 30 seconds");
}
