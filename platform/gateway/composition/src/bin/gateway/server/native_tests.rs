//! Empty profile capabilities through production authenticated MCP HTTP routing.
use super::*;
use rmcp::model::{
    CacheScope, CallToolRequestParams, ClientConfig, ResultType, SubscriptionFilter,
};
use veoveo_mcp_contract::{DiscoveryFailureMode, GatewayDiscoveryMetadata};

#[tokio::test]
async fn empty_profiles_advertise_and_serve_the_authenticated_tools_catalogue() {
    tokio::time::timeout(std::time::Duration::from_secs(180), async {
        for mode in [
            DiscoveryFailureMode::FailClosed,
            DiscoveryFailureMode::Isolate,
        ] {
            let (_db, auth, _key_file, key) = crate::auth::tests::profile_fixture(Some(mode)).await;
            let profile = GatewayProfileId::parse("operator").unwrap();
            assert!(auth.catalog.current().profile_servers(&profile).is_empty());
            let stop = CancellationToken::new();
            let state = DynamicMcpState {
                catalog: auth.catalog.clone(),
                gateway_state: auth.gateway_state.clone(),
                internal_token_issuer: GatewayInternalTokenIssuer::new(
                    TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap(),
                    GatewayInternalSigningKey::new("test", key.serialize_der()).unwrap(),
                ),
                upstream_http: GatewayUpstreamHttpClientPool::new(),
                allowed_hosts: Arc::new(vec!["computers.test".into()]),
                cancellation_token: stop.child_token(),
                services: Default::default(),
            };
            let router = Router::new()
                .route("/mcp/{profile}", any(dynamic_mcp_profile))
                .route("/mcp/{profile}/{*path}", any(dynamic_mcp_profile))
                .with_state(state)
                .layer(middleware::from_fn_with_state(
                    auth.clone(),
                    authenticate_mcp,
                ))
                .layer(middleware::from_fn(
                    veoveo_mcp_gateway::request_observation::observe_request,
                ));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = std::num::NonZeroU16::new(listener.local_addr().unwrap().port()).unwrap();
            let shutdown = stop.clone();
            let serving = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
                axum::serve(listener, router)
                    .with_graceful_shutdown(shutdown.cancelled_owned())
                    .await
                    .unwrap();
            }));
            let transport = veoveo_mcp_gateway::http::native_mcp::NativeTransport::new(
                port,
                &auth.deployment,
                ClientConfig::default(),
            )
            .unwrap();
            let bearer = crate::auth::tests::fixture_bearer(&key, 120);
            let client = transport.connect(&profile, &bearer).await.unwrap();
            let info = client.peer().peer_info().unwrap();
            assert!(info.capabilities.resources.is_none());
            assert!(info.capabilities.prompts.is_none());
            assert!(info.capabilities.completions.is_none());
            let catalogue = client.peer().list_tools(None).await.unwrap();
            assert!(catalogue.tools.is_empty());
            assert!(catalogue.next_cursor.is_none());
            assert_eq!(catalogue.result_type, Some(ResultType::COMPLETE));
            assert_eq!(catalogue.cache_scope, Some(CacheScope::Private));
            assert_eq!(
                catalogue.ttl_ms,
                Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS)
            );
            assert!(
                veoveo_mcp_gateway::http::native_mcp::tools(&client)
                    .await
                    .unwrap()
                    .is_empty()
            );
            let error = client
                .peer()
                .call_tool(CallToolRequestParams::new("missing__unknown"))
                .await
                .unwrap_err();
            assert!(matches!(error, rmcp::ServiceError::McpError(_)));
            let http = reqwest::Client::new();
            for method in ["server/discover", "tools/list"] {
                let response = http
                    .post(format!("http://127.0.0.1:{port}/mcp/operator"))
                    .header("host", "computers.test")
                    .json(
                        &serde_json::json!({"jsonrpc":"2.0", "id":1, "method":method, "params":{}}),
                    )
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            }
            assert!(
                info.capabilities.tools.is_some(),
                "gateway serves a complete empty tool catalogue"
            );
            assert_eq!(
                info.capabilities.tools.as_ref().unwrap().list_changed,
                (mode == DiscoveryFailureMode::Isolate).then_some(true)
            );
            if mode == DiscoveryFailureMode::Isolate {
                let filter = SubscriptionFilter::builder().tools_list_changed().build();
                let mut changes = client.peer().listen(filter.clone()).await.unwrap();
                assert_eq!(changes.acknowledged(), &filter);
                assert!(
                    tokio::time::timeout(std::time::Duration::from_millis(50), changes.next())
                        .await
                        .is_err()
                );
                changes.cancel().await.unwrap();
            }
            client.close().await;
            stop.cancel();
            tokio::time::timeout(std::time::Duration::from_secs(3), serving)
                .await
                .unwrap()
                .unwrap();
        }
    })
    .await
    .expect("empty profile native MCP qualification deadline");
}

/// A disabled tools surface must not even connect to its rejecting upstream.
#[tokio::test]
async fn tools_discovery_skips_disabled_backends_before_upstream_work() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use veoveo_gateway_contract::UpstreamUrl;
    use veoveo_mcp_contract::Exposure;
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        for mode in [
            DiscoveryFailureMode::FailClosed,
            DiscoveryFailureMode::Isolate,
        ] {
            for exposure in [Exposure::None, Exposure::Listed(vec![]), Exposure::All] {
                let (_db, auth, _key_file, key) = crate::auth::tests::profile_fixture(None).await;
                let requests = Arc::new(AtomicUsize::new(0));
                let observed = requests.clone();
                let rejecting = Router::new().fallback(any(move || {
                    let observed = observed.clone();
                    async move {
                        observed.fetch_add(1, Ordering::SeqCst);
                        StatusCode::FORBIDDEN
                    }
                }));
                let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = upstream.local_addr().unwrap();
                let stop = CancellationToken::new();
                let shutdown = stop.clone();
                let backend = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
                    axum::serve(upstream, rejecting)
                        .with_graceful_shutdown(shutdown.cancelled_owned())
                        .await
                        .unwrap();
                }));
                let mut control = auth.catalog.current().control_plane().clone();
                control.profiles[0].discovery_failure_mode = mode;
                control.profiles[0].servers[0].tools = exposure.clone();
                control.servers[0].upstream.url =
                    UpstreamUrl::parse(format!("http://{address}/mcp")).unwrap();
                // This manifest requests a watch: disabled exposure must skip that too.
                control.servers[0].capabilities.tools_list_changed = true;
                auth.catalog
                    .replace(Arc::new(
                        GatewayCatalog::from_control_plane(
                            control,
                            auth.catalog.current().admission(),
                        )
                        .unwrap(),
                    ))
                    .unwrap();
                let state = DynamicMcpState {
                    catalog: auth.catalog.clone(),
                    gateway_state: auth.gateway_state.clone(),
                    internal_token_issuer: GatewayInternalTokenIssuer::new(
                        TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap(),
                        GatewayInternalSigningKey::new("test", key.serialize_der()).unwrap(),
                    ),
                    upstream_http: GatewayUpstreamHttpClientPool::new(),
                    allowed_hosts: Arc::new(vec!["computers.test".into()]),
                    cancellation_token: stop.child_token(),
                    services: Default::default(),
                };
                let router = Router::new()
                    .route("/mcp/{profile}", any(dynamic_mcp_profile))
                    .route("/mcp/{profile}/{*path}", any(dynamic_mcp_profile))
                    .with_state(state)
                    .layer(middleware::from_fn_with_state(
                        auth.clone(),
                        authenticate_mcp,
                    ))
                    .layer(middleware::from_fn(
                        veoveo_mcp_gateway::request_observation::observe_request,
                    ));
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let port =
                    std::num::NonZeroU16::new(listener.local_addr().unwrap().port()).unwrap();
                let shutdown = stop.clone();
                let serving = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
                    axum::serve(listener, router)
                        .with_graceful_shutdown(shutdown.cancelled_owned())
                        .await
                        .unwrap();
                }));
                let transport = veoveo_mcp_gateway::http::native_mcp::NativeTransport::new(
                    port,
                    &auth.deployment,
                    ClientConfig::default(),
                )
                .unwrap();
                let profile = GatewayProfileId::parse("operator").unwrap();
                let client = transport
                    .connect(&profile, &crate::auth::tests::fixture_bearer(&key, 120))
                    .await
                    .unwrap();
                let result = client.peer().list_tools(None).await;
                if matches!(exposure, Exposure::All) {
                    if mode == DiscoveryFailureMode::FailClosed {
                        assert!(result.is_err(), "enabled backend failure must fail closed");
                    } else {
                        let page = result.unwrap();
                        let failures =
                            veoveo_gateway_contract::GatewayDiscoveryDegradation::from_meta(
                                page.meta.as_ref(),
                            )
                            .unwrap()
                            .failures;
                        assert_eq!(failures.len(), 1);
                        assert_eq!(failures[0].server.as_str(), "computers");
                        assert_eq!(
                            failures[0].surface,
                            veoveo_gateway_contract::GatewayDiscoverySurface::Tools
                        );
                    }
                    assert!(requests.load(Ordering::SeqCst) > 0);
                } else {
                    let page = result.unwrap();
                    assert!(page.tools.is_empty());
                    assert_eq!(page.result_type, Some(ResultType::COMPLETE));
                    assert!(
                        veoveo_gateway_contract::GatewayDiscoveryDegradation::from_meta(
                            page.meta.as_ref()
                        )
                        .unwrap()
                        .failures
                        .is_empty()
                    );
                    // Repeated discovery must also avoid cache/watch/background work.
                    assert!(
                        client
                            .peer()
                            .list_tools(None)
                            .await
                            .unwrap()
                            .tools
                            .is_empty()
                    );
                    assert_eq!(requests.load(Ordering::SeqCst), 0);
                }
                client.close().await;
                stop.cancel();
                tokio::time::timeout(std::time::Duration::from_secs(3), serving)
                    .await
                    .unwrap()
                    .unwrap();
                tokio::time::timeout(std::time::Duration::from_secs(3), backend)
                    .await
                    .unwrap()
                    .unwrap();
            }
        }
    })
    .await
    .expect("disabled tools native MCP qualification deadline");
}

#[derive(Clone, Default)]
struct VisibleToolBackend {
    filters: Arc<std::sync::Mutex<Vec<SubscriptionFilter>>>,
    tools: Arc<std::sync::atomic::AtomicUsize>,
}
impl rmcp::ServerHandler for VisibleToolBackend {
    fn get_info(&self) -> rmcp::model::ServerConfig {
        let mut config = rmcp::model::ServerConfig::default();
        config.capabilities = rmcp::model::ServerCapabilities::builder()
            .enable_tools()
            .enable_tool_list_changed()
            .enable_resources()
            .enable_resources_list_changed()
            .enable_prompts()
            .enable_prompts_list_changed()
            .build();
        config
    }
    async fn list_tools(
        &self,
        _: Option<rmcp::model::PaginatedRequestParams>,
        _: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, rmcp::ErrorData> {
        self.tools.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(rmcp::model::ListToolsResult {
            tools: vec![rmcp::model::Tool::new(
                "create",
                "Fixture tool",
                Arc::new(
                    serde_json::from_value(serde_json::json!({"type":"object","properties":{}}))
                        .unwrap(),
                ),
            )],
            ..Default::default()
        })
    }
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        Some(requested.clone())
    }
    async fn listen(
        &self,
        context: rmcp::service::SubscriptionContext,
    ) -> Result<(), rmcp::ErrorData> {
        self.filters
            .lock()
            .unwrap()
            .push(context.accepted().clone());
        context.cancelled().await;
        Ok(())
    }
    async fn list_resources(
        &self,
        _: Option<rmcp::model::PaginatedRequestParams>,
        _: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, rmcp::ErrorData> {
        Ok(Default::default())
    }
}

#[tokio::test]
async fn tools_discovery_keeps_visible_tools_beside_disabled_rejecting_owner() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use veoveo_gateway_contract::UpstreamUrl;
    use veoveo_mcp_contract::{Exposure, MountPath, ServerSlug};
    use veoveo_types::ResourceScheme;
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        for mode in [
            DiscoveryFailureMode::FailClosed,
            DiscoveryFailureMode::Isolate,
        ] {
            let (_db, auth, _key_file, key) = crate::auth::tests::profile_fixture(None).await;
            let requests = Arc::new(AtomicUsize::new(0));
            let observed = requests.clone();
            let service = StreamableHttpService::new(
                || Ok(VisibleToolBackend::default()),
                veoveo_mcp_contract::stateless_session_manager(),
                veoveo_mcp_contract::canonical_streamable_http_server_config()
                    .with_allowed_hosts(["127.0.0.1"]),
            );
            let router = Router::new()
                .route_service("/mcp", service)
                .fallback(any(move || {
                    let observed = observed.clone();
                    async move {
                        observed.fetch_add(1, Ordering::SeqCst);
                        StatusCode::FORBIDDEN
                    }
                }));
            let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = upstream.local_addr().unwrap();
            let stop = CancellationToken::new();
            let shutdown = stop.clone();
            let backend = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
                axum::serve(upstream, router)
                    .with_graceful_shutdown(shutdown.cancelled_owned())
                    .await
                    .unwrap();
            }));
            let mut control = auth.catalog.current().control_plane().clone();
            control.profiles[0].discovery_failure_mode = mode;
            control.servers[0].upstream.url =
                UpstreamUrl::parse(format!("http://{address}/mcp")).unwrap();
            control.servers[0].capabilities.resources_list_changed = false;
            let mut hidden = control.servers[0].clone();
            hidden.slug = ServerSlug::parse("hidden").unwrap();
            hidden.uri_scheme = ResourceScheme::parse("hidden").unwrap();
            hidden.mount_path = MountPath::new("/hidden").unwrap();
            hidden.mcp_path = MountPath::new("/hidden/mcp").unwrap();
            hidden.upstream.url = UpstreamUrl::parse(format!("http://{address}/disabled")).unwrap();
            hidden.capabilities.tools_list_changed = true;
            let mut exposure = control.profiles[0].servers[0].clone();
            exposure.server = hidden.slug.clone();
            exposure.tools = Exposure::None;
            exposure.resources = Exposure::None;
            exposure.prompts = Exposure::None;
            control.profiles[0].servers.push(exposure);
            control.servers.push(hidden);
            control.policies[0].rules[0]
                .actions
                .insert(veoveo_gateway_contract::GatewayAction::ToolsList.into());
            auth.catalog
                .replace(Arc::new(
                    GatewayCatalog::from_control_plane(control, auth.catalog.current().admission())
                        .unwrap(),
                ))
                .unwrap();
            let state = DynamicMcpState {
                catalog: auth.catalog.clone(),
                gateway_state: auth.gateway_state.clone(),
                internal_token_issuer: GatewayInternalTokenIssuer::new(
                    TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap(),
                    GatewayInternalSigningKey::new("test", key.serialize_der()).unwrap(),
                ),
                upstream_http: GatewayUpstreamHttpClientPool::new(),
                allowed_hosts: Arc::new(vec!["computers.test".into()]),
                cancellation_token: stop.child_token(),
                services: Default::default(),
            };
            let router = Router::new()
                .route("/mcp/{profile}", any(dynamic_mcp_profile))
                .route("/mcp/{profile}/{*path}", any(dynamic_mcp_profile))
                .with_state(state)
                .layer(middleware::from_fn_with_state(
                    auth.clone(),
                    authenticate_mcp,
                ))
                .layer(middleware::from_fn(
                    veoveo_mcp_gateway::request_observation::observe_request,
                ));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = std::num::NonZeroU16::new(listener.local_addr().unwrap().port()).unwrap();
            let shutdown = stop.clone();
            let serving = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
                axum::serve(listener, router)
                    .with_graceful_shutdown(shutdown.cancelled_owned())
                    .await
                    .unwrap();
            }));
            let transport = veoveo_mcp_gateway::http::native_mcp::NativeTransport::new(
                port,
                &auth.deployment,
                ClientConfig::default(),
            )
            .unwrap();
            let profile = GatewayProfileId::parse("operator").unwrap();
            let client = transport
                .connect(&profile, &crate::auth::tests::fixture_bearer(&key, 120))
                .await
                .unwrap();

            let page = client.peer().list_tools(None).await.unwrap();
            let tools = &page.tools;
            assert_eq!(tools.len(), 1);
            assert_eq!(tools[0].name, "computers__create");
            assert!(
                veoveo_gateway_contract::GatewayDiscoveryDegradation::from_meta(page.meta.as_ref())
                    .unwrap()
                    .failures
                    .is_empty()
            );
            assert_eq!(
                requests.load(Ordering::SeqCst),
                0,
                "disabled owner's watch/list must not be contacted"
            );
            client.close().await;
            stop.cancel();
            tokio::time::timeout(std::time::Duration::from_secs(3), serving)
                .await
                .unwrap()
                .unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(3), backend)
                .await
                .unwrap()
                .unwrap();
        }
    })
    .await
    .expect("mixed tools native MCP qualification deadline");
}

#[tokio::test]
async fn catalog_watches_and_explicit_routes_omit_disabled_surfaces() {
    use veoveo_gateway_contract::UpstreamUrl;
    use veoveo_mcp_contract::Exposure;
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        for mode in [
            DiscoveryFailureMode::FailClosed,
            DiscoveryFailureMode::Isolate,
        ] {
            let (_db, auth, _key_file, key) = crate::auth::tests::profile_fixture(None).await;
            let fixture = VisibleToolBackend::default();
            let owner = fixture.clone();
            let service = StreamableHttpService::new(
                move || Ok(owner.clone()),
                veoveo_mcp_contract::stateless_session_manager(),
                veoveo_mcp_contract::canonical_streamable_http_server_config()
                    .with_allowed_hosts(["127.0.0.1"]),
            );
            let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = upstream.local_addr().unwrap();
            let stop = CancellationToken::new();
            let shutdown = stop.clone();
            let backend = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
                axum::serve(upstream, Router::new().route_service("/mcp", service))
                    .with_graceful_shutdown(shutdown.cancelled_owned())
                    .await
                    .unwrap();
            }));
            let mut control = auth.catalog.current().control_plane().clone();
            control.profiles[0].discovery_failure_mode = mode;
            control.profiles[0].servers[0].tools = Exposure::None;
            control.profiles[0].servers[0].prompts = Exposure::None;
            control.profiles[0].servers[0].resources = Exposure::All;
            control.servers[0].upstream.url =
                UpstreamUrl::parse(format!("http://{address}/mcp")).unwrap();
            control.servers[0].capabilities.tools_list_changed = true;
            control.servers[0].capabilities.prompts_list_changed = true;
            control.servers[0].capabilities.resources_list_changed = true;
            auth.catalog
                .replace(Arc::new(
                    GatewayCatalog::from_control_plane(control, auth.catalog.current().admission())
                        .unwrap(),
                ))
                .unwrap();
            let state = DynamicMcpState {
                catalog: auth.catalog.clone(),
                gateway_state: auth.gateway_state.clone(),
                internal_token_issuer: GatewayInternalTokenIssuer::new(
                    TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap(),
                    GatewayInternalSigningKey::new("test", key.serialize_der()).unwrap(),
                ),
                upstream_http: GatewayUpstreamHttpClientPool::new(),
                allowed_hosts: Arc::new(vec!["computers.test".into()]),
                cancellation_token: stop.child_token(),
                services: Default::default(),
            };
            let router = Router::new()
                .route("/mcp/{profile}", any(dynamic_mcp_profile))
                .route("/mcp/{profile}/{*path}", any(dynamic_mcp_profile))
                .with_state(state)
                .layer(middleware::from_fn_with_state(
                    auth.clone(),
                    authenticate_mcp,
                ))
                .layer(middleware::from_fn(
                    veoveo_mcp_gateway::request_observation::observe_request,
                ));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = std::num::NonZeroU16::new(listener.local_addr().unwrap().port()).unwrap();
            let shutdown = stop.clone();
            let serving = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
                axum::serve(listener, router)
                    .with_graceful_shutdown(shutdown.cancelled_owned())
                    .await
                    .unwrap();
            }));
            let transport = veoveo_mcp_gateway::http::native_mcp::NativeTransport::new(
                port,
                &auth.deployment,
                ClientConfig::default(),
            )
            .unwrap();
            let profile = GatewayProfileId::parse("operator").unwrap();
            let client = transport
                .connect(&profile, &crate::auth::tests::fixture_bearer(&key, 120))
                .await
                .unwrap();

            assert!(
                client
                    .peer()
                    .list_tools(None)
                    .await
                    .unwrap()
                    .tools
                    .is_empty()
            );
            assert!(fixture.filters.lock().unwrap().is_empty());
            assert!(
                client
                    .peer()
                    .list_resources(None)
                    .await
                    .unwrap()
                    .resources
                    .is_empty()
            );
            {
                let filters = fixture.filters.lock().unwrap();
                assert!(
                    !filters.is_empty(),
                    "enabled resource cache requires its live watch"
                );
                assert!(
                    filters
                        .iter()
                        .all(|f| f.resources_list_changed == Some(true)
                            && f.tools_list_changed.is_none()
                            && f.prompts_list_changed.is_none())
                );
            }
            let count = fixture.filters.lock().unwrap().len();
            let filter = SubscriptionFilter::builder()
                .tools_list_changed()
                .prompts_list_changed()
                .build();
            let mut subscription = client.peer().listen(filter.clone()).await.unwrap();
            assert_eq!(subscription.acknowledged(), &filter);
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(150), subscription.next())
                    .await
                    .is_err()
            );
            assert_eq!(
                fixture.filters.lock().unwrap().len(),
                count,
                "empty selected route must not open upstream subscription"
            );
            assert_eq!(fixture.tools.load(std::sync::atomic::Ordering::SeqCst), 0);
            subscription.cancel().await.unwrap();
            client.close().await;
            stop.cancel();
            tokio::time::timeout(std::time::Duration::from_secs(3), serving)
                .await
                .unwrap()
                .unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(3), backend)
                .await
                .unwrap()
                .unwrap();
        }
    })
    .await
    .expect("surface-specific watch qualification deadline");
}
