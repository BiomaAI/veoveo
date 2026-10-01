use super::*;
use crate::{GatewayState, mcp::discovery::DiscoveryCacheKey, mcp::task_ownership_tests};
use rmcp::model::{Resource, ResourceTemplate, Tool};
use veoveo_mcp_contract::GatewayControlPlane;
use veoveo_types::PrincipalId;

#[derive(Clone)]
struct EndingCatalog(std::sync::Arc<tokio::sync::Notify>);

impl rmcp::ServerHandler for EndingCatalog {
    fn get_info(&self) -> rmcp::model::ServerConfig {
        rmcp::model::ServerConfig::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_resources()
                .enable_resources_list_changed()
                .build(),
        )
    }
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        Some(requested.clone())
    }
    async fn listen(&self, context: rmcp::service::SubscriptionContext) -> Result<(), McpError> {
        tokio::select! { _ = self.0.notified() => {}, _ = context.cancelled() => {} }
        Ok(())
    }
}

#[tokio::test]
async fn an_ended_catalog_source_reaches_the_downstream_reconnect_path() {
    use rmcp::{ClientServiceExt, ServiceExt};
    let end = std::sync::Arc::new(tokio::sync::Notify::new());
    let handler = EndingCatalog(end.clone());
    let (server_io, client_io) = tokio::io::duplex(8192);
    let server = tokio::spawn(async move { handler.serve(server_io).await.unwrap() });
    let mut client = ()
        .serve_with_lifecycle(
            client_io,
            rmcp::ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .unwrap();
    let mut server = server.await.unwrap();
    let subscription = client
        .listen(
            SubscriptionFilter::builder()
                .resources_list_changed()
                .build(),
        )
        .await
        .unwrap();
    let mut routed = Box::pin(routed_notifications(
        ServerSlug::new("catalog").unwrap(),
        false,
        subscription,
    ));
    end.notify_one();
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), routed.next())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event.2, Err(ServiceError::TransportClosed)));
    let _ = client
        .close_with_timeout(std::time::Duration::from_secs(2))
        .await;
    let _ = server
        .close_with_timeout(std::time::Duration::from_secs(2))
        .await;
}

fn key(server: &str) -> DiscoveryCacheKey {
    DiscoveryCacheKey {
        catalog_generation: 1,
        principal: PrincipalId::new("listener").unwrap(),
        authorization_fingerprint: [7; 32],
        server: ServerSlug::new(server).unwrap(),
    }
}

#[derive(Clone)]
struct DelayedBaseline {
    catalog: std::sync::Arc<tokio::sync::Notify>,
    first: std::sync::Arc<tokio::sync::Notify>,
    second: std::sync::Arc<tokio::sync::Notify>,
    close: bool,
}

impl rmcp::ServerHandler for DelayedBaseline {
    fn get_info(&self) -> rmcp::model::ServerConfig {
        rmcp::model::ServerConfig::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_resources()
                .enable_resources_subscribe()
                .enable_resources_list_changed()
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
        if self.close {
            return Ok(());
        }
        tokio::select! { _ = context.cancelled() => return Ok(()), _ = self.catalog.notified() => {} }
        context.sink().notify_resource_list_changed().await.unwrap();
        for (ready, uri) in [
            (&self.first, "fixture://one"),
            (&self.second, "fixture://two"),
        ] {
            if context.accepted().resource_subscriptions.is_none() {
                break;
            }
            tokio::select! { _ = context.cancelled() => return Ok(()), _ = ready.notified() => {} }
            context.sink().notify_resource_updated(uri).await.unwrap();
        }
        context.cancelled().await;
        Ok(())
    }
}

#[tokio::test]
async fn indexing_waits_for_every_root_and_preserves_catalog_changes_during_setup() {
    use rmcp::{ClientServiceExt, ServiceExt};
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        for (close, roots) in [(false, true), (true, true), (false, false), (true, false)] {
            let catalog = std::sync::Arc::new(tokio::sync::Notify::new());
            let first = std::sync::Arc::new(tokio::sync::Notify::new());
            let second = std::sync::Arc::new(tokio::sync::Notify::new());
            let handler = DelayedBaseline {
                catalog: catalog.clone(),
                first: first.clone(),
                second: second.clone(),
                close,
            };
            let (server_io, client_io) = tokio::io::duplex(8192);
            let server = tokio::spawn(async move { handler.serve(server_io).await.unwrap() });
            let client = ()
                .serve_with_lifecycle(
                    client_io,
                    rmcp::ClientLifecycleMode::Discover {
                        preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                    },
                )
                .await
                .unwrap();
            let server = server.await.unwrap();
            let mut filter = SubscriptionFilter::builder()
                .resources_list_changed()
                .resource_subscriptions(["fixture://one", "fixture://two"])
                .build();
            if !roots {
                filter.resource_subscriptions = None;
            }
            let mut subscription = client.listen(filter.clone()).await.unwrap();
            assert_eq!(subscription.acknowledged(), &filter);
            {
                let readiness = wait_for_source_baseline(&mut subscription, &filter);
                tokio::pin!(readiness);
                if close {
                    assert!(
                        readiness.await.is_err(),
                        "acknowledgement cannot prove observation readiness"
                    );
                } else {
                    assert!(
                        tokio::time::timeout(std::time::Duration::from_millis(25), &mut readiness)
                            .await
                            .is_err()
                    );
                    catalog.notify_one();
                    if roots {
                        first.notify_one();
                        assert!(
                            tokio::time::timeout(
                                std::time::Duration::from_millis(25),
                                &mut readiness
                            )
                            .await
                            .is_err()
                        );
                        second.notify_one();
                    }
                    assert!(
                        readiness.await.unwrap(),
                        "catalog change must survive baseline coalescing"
                    );
                }
            }
            drop(subscription);
            client.cancel().await.unwrap();
            server.cancel().await.unwrap();
        }
    })
    .await
    .expect("source readiness qualification exceeded 10 seconds");
}

#[tokio::test]
async fn subscription_list_changes_invalidate_cached_catalog_surfaces() {
    let db = crate::test_store::TestDb::new().await;
    let plane: GatewayControlPlane =
        serde_json::from_str(include_str!("../../../../../configs/gateway.local.json")).unwrap();
    let gateway = task_ownership_tests::gateway(GatewayState::new(db.a.clone()), plane);
    for server in ["media", "time"] {
        let key = key(server);
        let tools = gateway.discovery.start_tools(key.clone()).await;
        gateway
            .discovery
            .store_tools(
                tools,
                vec![Tool::new("before", "Before", std::sync::Arc::default())],
            )
            .await;
        let resources = gateway
            .discovery
            .begin(GatewayDiscoverySurface::Resources, key.clone())
            .await
            .unwrap();
        gateway
            .discovery
            .finish_resources(
                resources,
                vec![Resource::new(format!("{server}://before"), "before")],
            )
            .await;
        let templates = gateway
            .discovery
            .begin(GatewayDiscoverySurface::ResourceTemplates, key.clone())
            .await
            .unwrap();
        gateway
            .discovery
            .finish_resource_templates(
                templates,
                vec![ResourceTemplate::new(
                    format!("{server}://{{id}}"),
                    "before",
                )],
            )
            .await;
    }

    let server = ServerSlug::new("media").unwrap();
    let mut changed = ServerNotification::ResourceListChangedNotification(Default::default());
    gateway
        .project_subscription_notification(&server, None, &mut changed)
        .await
        .unwrap();
    assert!(gateway.discovery.resources(&key("media")).await.is_none());
    assert!(
        gateway
            .discovery
            .resource_templates(&key("media"))
            .await
            .is_none()
    );
    assert!(gateway.discovery.tools(&key("media")).await.is_some());
    assert!(gateway.discovery.resources(&key("time")).await.is_some());
    assert!(
        gateway
            .discovery
            .resource_templates(&key("time"))
            .await
            .is_some()
    );

    let mut changed = ServerNotification::ToolListChangedNotification(Default::default());
    gateway
        .project_subscription_notification(&server, None, &mut changed)
        .await
        .unwrap();
    assert!(gateway.discovery.tools(&key("media")).await.is_none());
    assert!(gateway.discovery.tools(&key("time")).await.is_some());

    // The first subsequent catalog read must discover again and expose new contents.
    let resources = gateway
        .discovery
        .begin(GatewayDiscoverySurface::Resources, key("media"))
        .await
        .unwrap();
    gateway
        .discovery
        .finish_resources(resources, vec![Resource::new("media://after", "after")])
        .await;
    assert_eq!(
        gateway
            .discovery
            .resources(&key("media"))
            .await
            .unwrap()
            .items[0]
            .uri,
        "media://after"
    );
}
