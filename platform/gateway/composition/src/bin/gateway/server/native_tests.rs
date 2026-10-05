//! Empty profile capabilities through production authenticated MCP HTTP routing.
use super::*;
use rmcp::model::{
    CacheScope, CallToolRequestParams, ClientConfig, ResultType, SubscriptionFilter,
};
use veoveo_mcp_contract::DiscoveryFailureMode;

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
