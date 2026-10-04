//! Synthetic protocol-checker qualification. Domain owners must separately
//! supply their real database/change-source restart and access fixtures.
use super::*;
use crate::{
    CheckStatus, ConformanceCredentials, KnowledgeRoute, KnowledgeSourceTarget,
    knowledge_probes::{KnowledgeChangeDriver, KnowledgeCreateDriver, KnowledgeProbeFuture},
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
const SECOND: &str = "fixture://reading/second";
const THIRD: &str = "fixture://reading/third";
const INDEX: &str = "fixture://readings";
const LEAK: u8 = 1;
const BAD_LINK: u8 = 2;
const LOST_STATE: u8 = 3;
const NO_CHANGE: u8 = 4;
const PARTIAL_ACK: u8 = 5;
const CONDITIONAL_LEAK: u8 = 6;
const BASELINE_ONLY: u8 = 7;
const NO_BASELINE: u8 = 8;
const REMOVED_READABLE: u8 = 9;
const REMOVED_ENUMERATED: u8 = 10;
const EXISTING_MEMBER: u8 = 11;
const CREATED_UNREADABLE: u8 = 12;
const CREATED_UNENUMERATED: u8 = 13;
const CREATED_CONDITIONAL_CONTENT: u8 = 14;
const REUSED_SECOND: u8 = 15;
const LOST_FIRST_AFTER_SECOND: u8 = 16;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Change {
    Update,
    Remove,
    Create,
}

fn descriptor() -> CollectionDescriptor {
    CollectionDescriptor::new(
        CollectionId::try_from("fixture.readings".to_owned()).unwrap(),
        EntityKind::parse("reading").unwrap(),
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
    route: KnowledgeRoute,
    change: Change,
}
impl State {
    fn visible(&self) -> Vec<&'static str> {
        let version = self.active.lock().unwrap().version;
        match self.change {
            Change::Update => vec![MEMBER],
            Change::Remove => match version {
                0 | 1 => vec![MEMBER, SECOND],
                2 => vec![SECOND],
                _ => vec![],
            },
            Change::Create => match version {
                0 | 1 => vec![MEMBER],
                2 => vec![MEMBER, SECOND],
                _ if self.fault.load(Ordering::Relaxed) == LOST_FIRST_AFTER_SECOND => {
                    vec![MEMBER, THIRD]
                }
                _ => vec![MEMBER, SECOND, THIRD],
            },
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.storage);
    }
}
impl KnowledgeChangeDriver for State {
    fn mutate(&self) -> KnowledgeProbeFuture<'_> {
        Box::pin(async move {
            let (hub, member) = {
                let mut source = self.active.lock().unwrap();
                let member = if self.change == Change::Remove && source.version >= 2 {
                    SECOND
                } else {
                    MEMBER
                };
                if self.fault.load(Ordering::Relaxed) != NO_CHANGE {
                    source.version += 1;
                    std::fs::write(&self.storage, source.version.to_string())?;
                }
                (source.hub.clone(), member)
            };
            hub.notify_resource_updated(member).await;
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

impl KnowledgeCreateDriver for State {
    fn create(&self) -> KnowledgeProbeFuture<'_, ResourceUri> {
        Box::pin(async move {
            let (hub, member) = {
                let mut source = self.active.lock().unwrap();
                let fault = self.fault.load(Ordering::Relaxed);
                if fault != NO_CHANGE {
                    source.version += 1;
                    std::fs::write(&self.storage, source.version.to_string())?;
                }
                let member = if fault == EXISTING_MEMBER {
                    MEMBER
                } else if source.version <= 2 || fault == REUSED_SECOND {
                    SECOND
                } else {
                    THIRD
                };
                (source.hub.clone(), member)
            };
            hub.notify_resource_updated(INDEX).await;
            ResourceUri::new(member).map_err(Into::into)
        })
    }
    fn restart(&self) -> KnowledgeProbeFuture<'_> {
        KnowledgeChangeDriver::restart(self)
    }
}

#[derive(Clone)]
struct Fixture(Arc<State>);
#[derive(Clone)]
struct Reader(u8);

async fn authenticate(mut request: Request, next: Next) -> axum::response::Response {
    let reader = match request
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
    {
        Some("Bearer ordinary") => Reader(1),
        Some("Bearer restricted") => Reader(0),
        Some("Bearer tool-denied") => Reader(2),
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
        == 1
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
            let visible = if self.0.fault.load(Ordering::Relaxed) == REMOVED_ENUMERATED {
                vec![MEMBER, SECOND]
            } else if self.0.fault.load(Ordering::Relaxed) == CREATED_UNENUMERATED {
                vec![MEMBER]
            } else {
                self.0.visible()
            };
            ReadResourceResult::new(vec![
                ResourceContents::text(
                    json!({"items": if permitted {visible.iter().map(|uri| json!({"uri": uri})).collect::<Vec<_>>()} else {vec![]}})
                        .to_string(),
                    INDEX,
                )
                .with_mime_type("application/json"),
            ])
        } else if [MEMBER, SECOND, THIRD].contains(&request.uri.as_str())
            && permitted
            && !(self.0.fault.load(Ordering::Relaxed) == CREATED_UNREADABLE
                && request.uri != MEMBER)
            && (self.0.visible().contains(&request.uri.as_str())
                || self.0.fault.load(Ordering::Relaxed) == REMOVED_READABLE
                || (self.0.fault.load(Ordering::Relaxed) == CONDITIONAL_LEAK
                    && knowledge::server::condition(Some(&context.meta))
                        .unwrap()
                        .is_some()))
        {
            let version = if self.0.change != Change::Update {
                1
            } else {
                self.0.active.lock().unwrap().version
            };
            let text = json!({"version": version}).to_string();
            let descriptor = descriptor();
            let observation = Observation::builder(
                descriptor.collection().clone(),
                Revision::parse(version.to_string()).unwrap(),
                knowledge::content_digest(&text),
                chrono::Utc::now(),
            )
            .build(&descriptor)
            .unwrap();
            knowledge::server::member_result(
                &ResourceUri::new(request.uri).unwrap(),
                "application/json",
                text,
                observation,
                &descriptor,
                if self.0.fault.load(Ordering::Relaxed) == CREATED_CONDITIONAL_CONTENT {
                    None
                } else {
                    Some(&context.meta)
                },
            )
            .unwrap()
        } else {
            return Err(ErrorData::invalid_request("resource is not readable", None));
        };
        Ok(veoveo_mcp_contract::private_resource_response(result, true))
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        let expected = match self.0.route {
            KnowledgeRoute::Direct => "search",
            KnowledgeRoute::Gateway => "fixture__search",
        };
        if request.name != expected {
            return Err(ErrorData::invalid_params("incorrect tool namespace", None));
        }
        if context
            .extensions
            .get::<axum::http::request::Parts>()
            .unwrap()
            .extensions
            .get::<Reader>()
            .unwrap()
            .0
            == 2
            && self.0.fault.load(Ordering::Relaxed) != LEAK
        {
            return Err(ErrorData::invalid_request("tool scope denied", None));
        }
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
        match self.0.fault.load(Ordering::Relaxed) {
            BASELINE_ONLY => {
                veoveo_mcp_contract::send_resource_update(
                    &context,
                    veoveo_mcp_contract::ResourceUpdate::Reconcile,
                )
                .await?;
                return Ok(());
            }
            NO_BASELINE => return Ok(()),
            _ => {}
        }
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
    for route in [KnowledgeRoute::Direct, KnowledgeRoute::Gateway] {
        for change in [Change::Update, Change::Remove, Change::Create] {
            tokio::time::timeout(Duration::from_secs(45), qualify(route, change)).await??;
        }
    }
    Ok(())
}

async fn qualify(route: KnowledgeRoute, change: Change) -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let state = Arc::new(State {
        active: Mutex::new(Source {
            version: 1,
            hub: Arc::new(SubscriptionHub::new()),
        }),
        storage: std::env::temp_dir()
            .join(format!("veoveo-knowledge-probe-{}", uuid::Uuid::now_v7())),
        fault: AtomicU8::new(0),
        route,
        change,
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
    let profile = KnowledgeSourceTarget::new(
        endpoint.parse()?,
        "fixture".parse()?,
        ["fixture".parse()?].into(),
        route,
    )?;
    let descriptor = descriptor();
    let mut tool = Tool::new(
        profile.tool_name(&LocalToolName::parse("search")?)?,
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
    let tools = profile.tools(if change == Change::Update {
        vec![tool]
    } else {
        vec![]
    })?;
    let probes = KnowledgeProbes {
        changes: vec![match change {
            Change::Remove => KnowledgeChangeProbe::remove(
                descriptors[0].collection().clone(),
                [ResourceUri::new(MEMBER)?, ResourceUri::new(SECOND)?],
                state.as_ref(),
            ),
            Change::Update => KnowledgeChangeProbe::update(
                descriptors[0].collection().clone(),
                ResourceUri::new(MEMBER)?,
                state.as_ref(),
            ),
            Change::Create => {
                KnowledgeChangeProbe::create(descriptors[0].collection().clone(), state.as_ref())
            }
        }],
        searches: if change != Change::Update {
            vec![]
        } else {
            vec![KnowledgeSearchProbe {
                tool: LocalToolName::parse("search")?,
                arguments: Default::default(),
                expected: BTreeSet::from([ResourceUri::new(MEMBER)?]),
                restricted: KnowledgeSearchAccess::Results(BTreeSet::new()),
                restricted_credentials: ConformanceCredentials::bearer("restricted"),
            }]
        },
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
            .any(|check| check.requirement_id == "K07" && check.status == CheckStatus::Failed)
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
            .all(|check| check.status != CheckStatus::Failed),
        "{checks:?}"
    );
    println!("{}", serde_json::to_string(&checks)?);
    if change == Change::Update {
        let denied = KnowledgeSearchProbe {
            tool: LocalToolName::parse("search")?,
            arguments: Default::default(),
            expected: [ResourceUri::new(MEMBER)?].into(),
            restricted_credentials: ConformanceCredentials::bearer("tool-denied"),
            restricted: KnowledgeSearchAccess::Denied,
        };
        let declaration = SearchDeclaration::new(vec![descriptors[0].collection().clone()])?;
        search(&client, &profile, &descriptors, &declaration, &denied).await?;
        state.fault.store(LEAK, Ordering::Relaxed);
        assert!(
            search(&client, &profile, &descriptors, &declaration, &denied)
                .await
                .is_err()
        );
        state.fault.store(0, Ordering::Relaxed);
    }
    let faults = if change == Change::Remove {
        vec![
            (LOST_STATE, "K07"),
            (NO_CHANGE, "K07"),
            (PARTIAL_ACK, "K07"),
            (CONDITIONAL_LEAK, "K07"),
            (BASELINE_ONLY, "K07"),
            (NO_BASELINE, "K07"),
            (REMOVED_READABLE, "K07"),
            (REMOVED_ENUMERATED, "K07"),
        ]
    } else if change == Change::Create {
        vec![
            (LOST_STATE, "K07"),
            (NO_CHANGE, "K07"),
            (PARTIAL_ACK, "K07"),
            (BASELINE_ONLY, "K07"),
            (NO_BASELINE, "K07"),
            (EXISTING_MEMBER, "K07"),
            (CREATED_UNREADABLE, "K07"),
            (CREATED_UNENUMERATED, "K07"),
            (CREATED_CONDITIONAL_CONTENT, "K07"),
            (REUSED_SECOND, "K07"),
            (LOST_FIRST_AFTER_SECOND, "K07"),
        ]
    } else {
        vec![
            (LEAK, "K08"),
            (BAD_LINK, "K08"),
            (LOST_STATE, "K07"),
            (NO_CHANGE, "K07"),
            (PARTIAL_ACK, "K07"),
            (CONDITIONAL_LEAK, "K08"),
            (BASELINE_ONLY, "K07"),
            (NO_BASELINE, "K07"),
        ]
    };
    for (fault, requirement) in faults {
        *state.active.lock().unwrap() = Source {
            version: 1,
            hub: Arc::new(SubscriptionHub::new()),
        };
        std::fs::write(&state.storage, "1")?;
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
