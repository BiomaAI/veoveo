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
use veoveo_gateway_contract::GatewayDiscoveryDegradation;
use veoveo_gateway_contract::{
    GatewayDiscoveryFailure, GatewayDiscoveryFailureCode, GatewayDiscoverySurface,
};
use veoveo_knowledge_contract::{CollectionApproval, KnowledgeCollectionApproval};
use veoveo_knowledge_mcp::source::{
    ApprovedCollection, DiscoveryScope, GatewaySource, ObservableSource, SourceListener,
};
use veoveo_mcp_contract::GatewayDiscoveryMetadata;
use veoveo_mcp_contract::SubscriptionHub;
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
    ContradictoryKnowledge,
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
        let mut profile: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../mcp/contract/testdata/compliance-example.json"
        )))
        .unwrap();
        profile["server"] = serde_json::json!(if matches!(
            *self.0.mode.lock().unwrap(),
            Mode::ForeignContract
        ) {
            "foreign"
        } else {
            "fixture"
        });
        if matches!(*self.0.mode.lock().unwrap(), Mode::ContradictoryKnowledge) {
            let item = profile["compliance"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|item| item["id"] == "C32")
                .unwrap();
            item["status"] = serde_json::json!("not_applicable");
            item["note"] = serde_json::json!("Knowledge extension is not declared.");
        }
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::new(
            serde_json::from_value(profile).unwrap(),
        );
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
            Mode::ContradictoryKnowledge,
        ] {
            *state.mode.lock().unwrap() = mode;
            let outcome = source.discover(&scope()).await;
            assert!(outcome.is_err());
            if matches!(mode, Mode::ContradictoryKnowledge) {
                let Err(error) = outcome else { unreachable!() };
                assert!(
                    error
                        .to_string()
                        .contains("contradicts its knowledge collection")
                );
            }
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

#[derive(Clone, Copy)]
enum MemberMode {
    TimeDesign,
    Boundary,
    Oversized,
    WrongUri,
    Duplicate,
    WrongDigest,
}
#[derive(Clone)]
struct MemberGateway(Arc<Mutex<MemberMode>>);
impl ServerHandler for MemberGateway {
    fn get_info(&self) -> ServerConfig {
        let mut capabilities = ServerCapabilities::builder().enable_resources().build();
        knowledge::server::declare(&mut capabilities);
        ServerConfig::new(capabilities)
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let mode = *self.0.lock().unwrap();
        let text = match mode {
            MemberMode::TimeDesign => include_str!("../../time-mcp/DESIGN.md").to_owned(),
            MemberMode::Oversized => {
                "x".repeat(veoveo_knowledge_contract::MAX_SOURCE_MEMBER_BYTES + 1)
            }
            _ => "x".repeat(veoveo_knowledge_contract::MAX_SOURCE_MEMBER_BYTES),
        };
        let observation = Observation::builder(
            descriptor().collection().clone(),
            "1".parse().unwrap(),
            content_digest(&text),
            "2026-10-10T00:00:00Z".parse().unwrap(),
        )
        .build(&descriptor())
        .unwrap();
        let mut result = knowledge::server::member_result(
            &ResourceUri::new(&request.uri).unwrap(),
            "text/markdown",
            text,
            observation,
            &descriptor(),
            Some(&context.meta),
        )
        .unwrap();
        match mode {
            MemberMode::WrongUri => {
                if let ResourceContents::TextResourceContents { uri, .. } = &mut result.contents[0]
                {
                    *uri = "fixture://records/foreign".into();
                }
            }
            MemberMode::Duplicate => result.contents.push(result.contents[0].clone()),
            MemberMode::WrongDigest => {
                if let ResourceContents::TextResourceContents { text, .. } = &mut result.contents[0]
                {
                    text.replace_range(..1, "y");
                }
            }
            _ => (),
        }
        Ok(result.into())
    }
}

#[tokio::test]
async fn source_member_bound_admits_full_time_design_and_rejects_invalid_reads() {
    use veoveo_knowledge_mcp::source::{KnowledgeSource, SourceRead};
    tokio::time::timeout(Duration::from_secs(30), async {
        let mode = Arc::new(Mutex::new(MemberMode::TimeDesign));
        let (server_io, client_io) = tokio::io::duplex(64 * 1024);
        let handler = MemberGateway(mode.clone());
        let serving = tokio::spawn(async move { handler.serve(server_io).await.unwrap() });
        let client = ()
            .serve_with_lifecycle(
                client_io,
                rmcp::ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            )
            .await
            .unwrap();
        let server = serving.await.unwrap();
        let source = GatewaySource::from_authenticated_peer(client.peer().clone());
        let uri = ResourceUri::new("fixture://records/one").unwrap();
        let SourceRead::Modified(document) =
            source.read(&descriptor(), uri.clone(), None).await.unwrap()
        else {
            panic!("full read must return source bytes");
        };
        assert_eq!(document.text(), include_str!("../../time-mcp/DESIGN.md"));
        assert!(document.text().len() > 64 * 1024);
        let ranges = veoveo_knowledge_mcp::chunk::ranges(
            document.text(),
            &veoveo_knowledge_contract::ChunkSettings::new("structure-v1", 1500, 150).unwrap(),
        )
        .unwrap();
        assert!(!ranges.is_empty() && ranges.len() <= 256);
        assert_eq!(ranges.last().unwrap().end, document.text().len());
        *mode.lock().unwrap() = MemberMode::Boundary;
        let SourceRead::Modified(document) =
            source.read(&descriptor(), uri.clone(), None).await.unwrap()
        else {
            panic!("full read required")
        };
        assert_eq!(
            document.text().len(),
            veoveo_knowledge_contract::MAX_SOURCE_MEMBER_BYTES
        );
        for invalid in [
            MemberMode::Oversized,
            MemberMode::WrongUri,
            MemberMode::Duplicate,
            MemberMode::WrongDigest,
        ] {
            *mode.lock().unwrap() = invalid;
            assert!(source.read(&descriptor(), uri.clone(), None).await.is_err());
        }
        client.cancel().await.unwrap();
        server.cancel().await.unwrap();
    })
    .await
    .expect("member read control exceeded 30 seconds");
}
