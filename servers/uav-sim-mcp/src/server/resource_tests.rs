//! Resource authority and routing checks use native Store and a deterministic adapter.
use super::*;
use crate::{
    adapter::{Adapter, FakeAdapter},
    server::test_support,
};
use rmcp::ServerHandler;
use std::time::Duration;
use tokio::sync::Mutex;

#[test]
fn resource_scope_matrix_preserves_read_control_admin_and_stream_requirements() {
    for mask in 0..16 {
        let mut identity = test_support::identity("scope-test", "operations", "pilot", &[]);
        identity.actor.scopes = UavScope::ALL
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, scope)| (*scope).into())
            .collect();
        identity
            .actor
            .scopes
            .insert(veoveo_types::ScopeName::parse("external:custom").unwrap());
        let has = |scope| identity_has_scope(&identity, scope);
        let read = has(UavScope::Read) || has(UavScope::Control) || has(UavScope::Admin);
        let control = has(UavScope::Control) || has(UavScope::Admin);
        for (uri, allowed) in [
            (uris::DOCS, true),
            (uris::CONTRACT, true),
            (uris::LIVE_APP_URI, has(UavScope::Stream)),
            (uris::SESSIONS, read),
            (uris::MISSIONS, read),
            (uris::USAGE, read),
            (uris::CONTROL_GRANTS, control),
            (uris::MISSION_PLANS, control),
            ("uav-sim://session/s/world", read),
            (
                "uav-sim://session/s/live-cameras",
                read && has(UavScope::Stream),
            ),
            (
                "uav-sim://session/s/live-view/v",
                read && has(UavScope::Stream),
            ),
        ] {
            assert_eq!(
                require_resource_scope(&identity, &UavResource::parse(uri).unwrap()).is_ok(),
                allowed,
                "mask {mask}: {uri}"
            );
        }
    }
}

#[tokio::test]
async fn reads_and_subscription_admission_reject_bad_routes_scopes_and_parents() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = test_support::fixture::TestDb::new().await;
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut simulation = fake_state().unwrap();
        simulation.session_id = SessionId::parse("native-session").unwrap();
        let adapter = Arc::new(Mutex::new(FakeAdapter::new(simulation.clone())));
        let state = test_support::state(
            &db.a,
            Arc::new(Adapter::Fake(adapter.clone())),
            "resource-test",
        );
        let mut running = rmcp::service::serve_directly(
            crate::server::service::hosted(state.clone()),
            (futures::sink::drain(), futures::stream::pending()),
            None,
        );
        let server = running.service();
        for uri in [
            uris::MISSIONS,
            uris::USAGE,
            uris::CONTROL_GRANTS,
            uris::MISSION_PLANS,
            "uav-sim://session/native-session/world",
            "uav-sim://session/native-session/vehicle/uav-1",
        ] {
            let context = test_support::context(running.peer(), &[UavScope::Admin], false);
            server
                .read_resource(ReadResourceRequestParams::new(uri), context.clone())
                .await
                .unwrap();
            subscribe(server, uri, &context).await.unwrap();
        }
        for uri in [
            "uav-sim://session/other/world",
            "uav-sim://session/native-session/vehicle/absent",
            "uav-sim://control-grant/absent",
            "uav-sim://mission-plan/absent",
            "uav-sim://mission/absent",
            "uav-sim://usage/task/0195e2ec-54a1-7000-8000-000000000001",
            "uav-sim://session/native-session/world?offset=1",
            "uav-sim://session/../world",
            "uav-sim://missions?cursor=00&cursor=00",
        ] {
            let context =
                test_support::context(running.peer(), &[UavScope::Admin, UavScope::Stream], false);
            assert!(
                server
                    .read_resource(ReadResourceRequestParams::new(uri), context.clone())
                    .await
                    .is_err(),
                "{uri}"
            );
            assert!(subscribe(server, uri, &context).await.is_err(), "{uri}");
        }
        for (uri, scopes) in [
            (uris::CONTROL_GRANTS, vec![UavScope::Read]),
            (uris::USAGE, vec![UavScope::Stream]),
            (
                "uav-sim://session/native-session/live-cameras",
                vec![UavScope::Read],
            ),
            (
                "uav-sim://session/native-session/live-cameras",
                vec![UavScope::Stream],
            ),
        ] {
            let context = test_support::context(running.peer(), &scopes, false);
            assert!(
                server
                    .read_resource(ReadResourceRequestParams::new(uri), context.clone())
                    .await
                    .is_err()
            );
            assert!(subscribe(server, uri, &context).await.is_err());
        }
        let cursor =
            UavMissionCursor::new(crate::contract::MissionId::parse("last").unwrap()).unwrap();
        let address = serde_json::to_value(UavResource::Missions {
            cursor: Some(cursor),
        })
        .unwrap();
        let uri = address.as_str().unwrap();
        let context = test_support::context(running.peer(), &[UavScope::Read], false);
        server
            .read_resource(ReadResourceRequestParams::new(uri), context.clone())
            .await
            .unwrap();
        assert!(subscribe(server, uri, &context).await.is_err());
        let identity = test_support::identity("scope-test", "operations", "pilot", &[]);
        let connection = state
            .live_views
            .open(
                crate::server::ownership::live_view_owner(&identity),
                identity.audit_context().unwrap(),
                crate::contract::OpenLiveViewRequest {
                    session_id: LiveSessionId::parse("native-session").unwrap(),
                    camera_id: simulation.live_cameras[0].camera_id.clone(),
                    viewer_instance_id: crate::contract::LiveViewerInstanceId::parse(
                        "resource-viewer",
                    )
                    .unwrap(),
                },
            )
            .await
            .unwrap();
        let context =
            test_support::context(running.peer(), &[UavScope::Read, UavScope::Stream], false);
        server
            .read_resource(
                ReadResourceRequestParams::new(connection.stream.resource_uri.as_str()),
                context.clone(),
            )
            .await
            .unwrap();
        subscribe(server, connection.stream.resource_uri.as_str(), &context)
            .await
            .unwrap();
        simulation.session_id = SessionId::parse("other").unwrap();
        *adapter.lock().await = FakeAdapter::new(simulation);
        let wrong_parent = uris::live_view(
            &LiveSessionId::parse("other").unwrap(),
            &connection.stream.live_view_id,
        );
        let error = server
            .read_resource(
                ReadResourceRequestParams::new(wrong_parent.as_str()),
                context.clone(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.message, "live view not found in session");
        assert!(
            subscribe(server, wrong_parent.as_str(), &context)
                .await
                .is_err()
        );
        running.close().await.unwrap();
    })
    .await
    .expect("native resource admission exceeded 60 seconds");
}

/// Admits one subscribed URI the way the host does: parse, then authorize.
async fn subscribe(
    server: &crate::server::service::HostedUav,
    uri: &str,
    context: &rmcp::service::RequestContext<rmcp::RoleServer>,
) -> Result<(), rmcp::ErrorData> {
    let addresses = veoveo_mcp_contract::hosting::requested_addresses::<
        crate::contract::UavResource,
    >(Some(&[uri.to_owned()]))?;
    veoveo_mcp_contract::hosting::ResourceSubscriptions::authorize(
        server.domain(),
        addresses,
        context,
    )
    .await
}
