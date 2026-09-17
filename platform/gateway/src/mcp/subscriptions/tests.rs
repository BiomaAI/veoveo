use super::*;
use crate::{GatewayState, mcp::discovery::DiscoveryCacheKey, mcp::task_ownership_tests};
use rmcp::model::{Resource, ResourceTemplate, Tool};
use veoveo_mcp_contract::{GatewayControlPlane, PrincipalId};

fn key(server: &str) -> DiscoveryCacheKey {
    DiscoveryCacheKey {
        catalog_generation: 1,
        principal: PrincipalId::new("listener").unwrap(),
        authorization_fingerprint: [7; 32],
        server: ServerSlug::new(server).unwrap(),
    }
}

#[tokio::test]
async fn subscription_list_changes_invalidate_cached_catalog_surfaces() {
    let db = crate::test_store::TestDb::new().await;
    let plane: GatewayControlPlane =
        serde_json::from_str(include_str!("../../../../../configs/gateway.local.json")).unwrap();
    let gateway = task_ownership_tests::gateway(GatewayState::new(db.a.clone()), plane);
    for server in ["media", "time"] {
        let key = key(server);
        gateway
            .discovery
            .store_tools(
                key.clone(),
                vec![Tool::new("before", "Before", std::sync::Arc::default())],
            )
            .await;
        assert!(
            gateway
                .discovery
                .begin(GatewayDiscoverySurface::Resources, key.clone())
                .await
        );
        gateway
            .discovery
            .finish_resources(
                key.clone(),
                vec![Resource::new(format!("{server}://before"), "before")],
            )
            .await;
        assert!(
            gateway
                .discovery
                .begin(GatewayDiscoverySurface::ResourceTemplates, key.clone())
                .await
        );
        gateway
            .discovery
            .finish_resource_templates(
                key,
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
    assert!(
        gateway
            .discovery
            .begin(GatewayDiscoverySurface::Resources, key("media"))
            .await
    );
    gateway
        .discovery
        .finish_resources(key("media"), vec![Resource::new("media://after", "after")])
        .await;
    assert_eq!(
        gateway.discovery.resources(&key("media")).await.unwrap()[0].uri,
        "media://after"
    );
}
