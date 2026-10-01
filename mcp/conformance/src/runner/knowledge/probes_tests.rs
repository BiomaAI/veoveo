//! Synthetic protocol-checker qualification. Domain owners must separately
//! supply their real database/change-source restart and access fixtures.
use super::*;
use crate::{
    CheckStatus, ConformanceCredentials, HostedServerProfileSchema, HttpBoundaryProfile,
    SurfaceExpectation, SurfaceProfile,
    knowledge_probes::{KnowledgeChangeDriver, KnowledgeProbeFuture},
};
use axum::{Router, extract::Request, http::StatusCode, middleware::Next, response::IntoResponse};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, Implementation, ReadResourceRequestParams,
        ReadResourceResponse, ReadResourceResult, ResourceContents, ServerCapabilities,
        ServerConfig,
    },
    service::{RequestContext, SubscriptionContext},
    transport::streamable_http_server::StreamableHttpService,
};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    },
};
use veoveo_mcp_contract::subscriptions::{SubscriptionHub, listen_resources};
use veoveo_mcp_knowledge_extension::{
    AccessModel, CollectionId, EntityKind, Freshness, IndexingMode, Revision, SearchHit,
};
use veoveo_types::{LocalToolName, ResourceTemplateUri};

const MEMBER: &str = "fixture://reading/member";
const INDEX: &str = "fixture://readings";
const LEAK: u8 = 1;
const BAD_LINK: u8 = 2;
const LOST_STATE: u8 = 3;
const NO_CHANGE: u8 = 4;
const PARTIAL_ACK: u8 = 5;
const CONDITIONAL_LEAK: u8 = 6;

fn descriptor() -> CollectionDescriptor {
    CollectionDescriptor::new(
        CollectionId::try_from("fixture.readings".to_owned()).unwrap(),
        EntityKind::new("reading").unwrap(),
        ResourceTemplateUri::new(INDEX).unwrap(),
        Freshness::max_age(30),
        ChangeSignal::Listen,
        AccessModel::Profile,
        IndexingMode::Content,
    )
    .unwrap()
}

struct Source {
    version: u64,
    hub: Arc<SubscriptionHub>,
}
struct State {
    active: Mutex<Source>,
    storage: PathBuf,
    fault: AtomicU8,
}
impl Drop for State {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.storage);
    }
}
impl KnowledgeChangeDriver for State {
    fn mutate(&self) -> KnowledgeProbeFuture<'_> {
        Box::pin(async move {
            let hub = {
                let mut source = self.active.lock().unwrap();
                if self.fault.load(Ordering::Relaxed) != NO_CHANGE {
                    source.version += 1;
                    std::fs::write(&self.storage, source.version.to_string())?;
                }
                source.hub.clone()
            };
            hub.notify_resource_updated(MEMBER).await;
            hub.notify_resource_updated(INDEX).await;
            Ok(())
        })
    }
    fn restart(&self) -> KnowledgeProbeFuture<'_> {
        Box::pin(async move {
            let version = if self.fault.load(Ordering::Relaxed) == LOST_STATE {
                0
            } else {
                std::fs::read_to_string(&self.storage)?.parse()?
            };
            *self.active.lock().unwrap() = Source {
                version,
                hub: Arc::new(SubscriptionHub::new()),
            };
            Ok(())
        })
    }
}

#[derive(Clone)]
struct Fixture(Arc<State>);
#[derive(Clone)]
struct Reader(bool);

async fn authenticate(mut request: Request, next: Next) -> axum::response::Response {
    let reader = match request
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
    {
        Some("Bearer ordinary") => Reader(true),
        Some("Bearer restricted") => Reader(false),
        _ => return StatusCode::UNAUTHORIZED.into_response(),
    };
    request.extensions_mut().insert(reader);
    next.run(request).await
}
fn reader(context: &RequestContext<RoleServer>) -> bool {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .unwrap()
        .extensions
        .get::<Reader>()
        .unwrap()
        .0
}

impl ServerHandler for Fixture {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::default();
        info.server_info = Implementation::new("fixture", "1");
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_resources_subscribe()
            .build();
        knowledge::server::declare(&mut info.capabilities);
        info
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<ReadResourceResponse, ErrorData> {
        let permitted = reader(&context)
            || (self.0.fault.load(Ordering::Relaxed) == CONDITIONAL_LEAK
                && knowledge::server::condition(Some(&context.meta))
                    .unwrap()
                    .is_some());
        let result = if request.uri == INDEX {
            ReadResourceResult::new(vec![
                ResourceContents::text(
                    json!({"items": if permitted {vec![json!({"uri": MEMBER})]} else {vec![]}})
                        .to_string(),
                    INDEX,
                )
                .with_mime_type("application/json"),
            ])
        } else if request.uri == MEMBER && permitted {
            let version = self.0.active.lock().unwrap().version;
            let text = json!({"version": version}).to_string();
            let descriptor = descriptor();
            let observation = Observation::builder(
                descriptor.collection().clone(),
                Revision::new(version.to_string()).unwrap(),
                knowledge::content_digest(&text),
                chrono::Utc::now(),
            )
            .build(&descriptor)
            .unwrap();
            knowledge::server::member_result(
                &ResourceUri::new(MEMBER).unwrap(),
                "application/json",
                text,
                observation,
                &descriptor,
                Some(&context.meta),
            )
            .unwrap()
        } else {
            return Err(ErrorData::invalid_request("resource is not readable", None));
        };
        Ok(veoveo_mcp_contract::private_resource_response(result, true))
    }
    async fn call_tool(
        &self,
        _request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        let permitted = reader(&context) || self.0.fault.load(Ordering::Relaxed) == LEAK;
        let hits = if permitted {
            vec![
                SearchHit::new(
                    ResourceUri::new(MEMBER).unwrap(),
                    Some("Fixture".into()),
                    Some("Visible member".into()),
                    None,
                )
                .unwrap(),
            ]
        } else {
            vec![]
        };
        let mut result = knowledge::server::search_result(SearchResults::new(hits).unwrap());
        if permitted && self.0.fault.load(Ordering::Relaxed) == BAD_LINK {
            result.content.clear();
        }
        Ok(result.into())
    }
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        if self.0.fault.load(Ordering::Relaxed) == PARTIAL_ACK {
            Some(
                SubscriptionFilter::builder()
                    .resource_subscription(MEMBER)
                    .build(),
            )
        } else {
            Some(requested.clone())
        }
    }
    async fn listen(&self, context: SubscriptionContext) -> std::result::Result<(), ErrorData> {
        let hub = self.0.active.lock().unwrap().hub.clone();
        listen_resources(context, &hub, None).await
    }
}

struct OwnedListener(tokio::task::JoinHandle<()>);
impl Drop for OwnedListener {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
async fn owner_probes_observe_changes_restart_search_and_denial() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(45), qualify()).await??;
    Ok(())
}

async fn qualify() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let state = Arc::new(State {
        active: Mutex::new(Source {
            version: 1,
            hub: Arc::new(SubscriptionHub::new()),
        }),
        storage: std::env::temp_dir()
            .join(format!("veoveo-knowledge-probe-{}", uuid::Uuid::now_v7())),
        fault: AtomicU8::new(0),
    });
    std::fs::write(&state.storage, "1")?;
    let fixture = Fixture(state.clone());
    let service = StreamableHttpService::new(
        move || Ok(fixture.clone()),
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config(),
    );
    let router = Router::new()
        .nest_service("/mcp", service)
        .layer(axum::middleware::from_fn(authenticate));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = OwnedListener(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    let endpoint = format!("http://{address}/mcp");
    let client = CertificationClient
        .serve_with_lifecycle(
            StreamableHttpClientTransport::from_config(
                StreamableHttpClientTransportConfig::with_uri(endpoint.clone())
                    .auth_header("ordinary"),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await?;
    let profile = HostedServerConformanceProfile {
        schema_version: HostedServerProfileSchema::V1,
        profile_id: "knowledge-probes".into(),
        contract_revision: crate::HOSTED_MCP_CONTRACT_REVISION.into(),
        endpoint,
        server_slug: "fixture".into(),
        owned_resource_schemes: BTreeSet::from(["fixture".into()]),
        http: HttpBoundaryProfile {
            require_authentication_rejection: true,
            rejected_host: None,
            health_url: None,
            readiness_url: None,
            docs_llms_url: format!("http://{address}/docs/llms.txt"),
        },
        surfaces: SurfaceProfile {
            tools: SurfaceExpectation::Required,
            resources: SurfaceExpectation::Required,
            resource_templates: SurfaceExpectation::Required,
            prompts: SurfaceExpectation::Forbidden,
            completions: SurfaceExpectation::Forbidden,
            tasks: SurfaceExpectation::Forbidden,
            subscriptions: SurfaceExpectation::Required,
            required_tools: BTreeSet::new(),
            required_resources: BTreeSet::new(),
            required_resource_templates: BTreeSet::new(),
            required_prompts: BTreeSet::new(),
        },
    };
    let descriptor = descriptor();
    let mut tool = Tool::new(
        "search",
        "Search the fixture",
        serde_json::from_value::<rmcp::model::JsonObject>(
            json!({"type": "object", "additionalProperties": false}),
        )
        .unwrap(),
    );
    knowledge::server::attach_search(
        &mut tool,
        &SearchDeclaration::new(vec![descriptor.collection().clone()])?,
    );
    let descriptors = [descriptor];
    let tools = [tool];
    let probes = KnowledgeProbes {
        changes: vec![KnowledgeChangeProbe {
            collection: descriptors[0].collection().clone(),
            member: ResourceUri::new(MEMBER)?,
            driver: state.as_ref(),
        }],
        searches: vec![KnowledgeSearchProbe {
            tool: LocalToolName::new("search")?,
            arguments: Default::default(),
            expected: BTreeSet::from([ResourceUri::new(MEMBER)?]),
            restricted_expected: BTreeSet::new(),
            restricted_credentials: ConformanceCredentials::bearer("restricted"),
        }],
    };
    let mut checks = Vec::new();
    check(
        &client,
        &profile,
        &descriptors,
        &tools,
        &KnowledgeProbes::default(),
        &mut checks,
    )
    .await;
    assert!(
        checks
            .iter()
            .all(|check| check.status == CheckStatus::Failed)
    );
    checks.clear();
    check(
        &client,
        &profile,
        &descriptors,
        &tools,
        &probes,
        &mut checks,
    )
    .await;
    assert_eq!(checks.len(), 2);
    assert!(
        checks
            .iter()
            .all(|check| check.status == CheckStatus::Passed),
        "{checks:?}"
    );
    println!("{}", serde_json::to_string(&checks)?);
    for (fault, requirement) in [
        (LEAK, "K08"),
        (BAD_LINK, "K08"),
        (LOST_STATE, "K07"),
        (NO_CHANGE, "K07"),
        (PARTIAL_ACK, "K07"),
        (CONDITIONAL_LEAK, "K08"),
    ] {
        state.fault.store(fault, Ordering::Relaxed);
        checks.clear();
        check(
            &client,
            &profile,
            &descriptors,
            &tools,
            &probes,
            &mut checks,
        )
        .await;
        assert!(
            checks
                .iter()
                .any(|check| check.requirement_id == requirement
                    && check.status == CheckStatus::Failed),
            "fault {fault}: {checks:?}"
        );
    }
    client.cancel().await?;
    server.0.abort();
    drop(server);
    Ok(())
}
