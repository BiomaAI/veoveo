use super::*;
use crate::contract::*;
use std::collections::{BTreeMap, BTreeSet};
use veoveo_types::{ResourceAddress, ScopeDefinition};

fn variables() -> BTreeMap<String, String> {
    [
        ("doc_id", "agents"),
        ("session_id", "Session_1.2"),
        ("vehicle_id", "vehicle-1"),
        ("camera_id", "camera-1"),
        ("product_id", "product-1"),
        ("live_view_id", "view-1"),
        ("mission_id", "mission-1"),
        ("grant_id", "grant-1"),
        ("plan_id", "plan-1"),
        ("task_id", "0195e2ec-54a1-7000-8000-000000000001"),
    ]
    .into_iter()
    .map(|(name, value)| (name.into(), value.into()))
    .collect()
}

fn template_cases() -> Vec<(&'static str, UavResource)> {
    let session = SessionId::parse("Session_1.2").unwrap();
    let live = LiveSessionId::parse("Session_1.2").unwrap();
    vec![
        (
            uris::CONTROL_GRANTS_PAGE_TEMPLATE,
            UavResource::ControlGrants { cursor: None },
        ),
        (
            uris::MISSION_PLANS_PAGE_TEMPLATE,
            UavResource::MissionPlans { cursor: None },
        ),
        (
            uris::MISSIONS_PAGE_TEMPLATE,
            UavResource::Missions { cursor: None },
        ),
        (
            uris::USAGE_PAGE_TEMPLATE,
            UavResource::Usage { cursor: None },
        ),
        (
            uris::DOC_TEMPLATE,
            UavResource::Document(UavDocument::Agents),
        ),
        (
            uris::SESSION_TEMPLATE,
            UavResource::Session(session.clone()),
        ),
        (uris::WORLD_TEMPLATE, UavResource::World(session.clone())),
        (uris::TILES_TEMPLATE, UavResource::Tiles(session.clone())),
        (
            uris::VEHICLES_TEMPLATE,
            UavResource::Vehicles(session.clone()),
        ),
        (
            uris::RECORDINGS_TEMPLATE,
            UavResource::Recordings(session.clone()),
        ),
        (
            uris::VEHICLE_TEMPLATE,
            UavResource::Vehicle {
                session,
                vehicle: VehicleId::parse("vehicle-1").unwrap(),
            },
        ),
        (
            uris::LIVE_CAMERAS_TEMPLATE,
            UavResource::LiveCameras(live.clone()),
        ),
        (
            uris::LIVE_CAMERA_TEMPLATE,
            UavResource::LiveCamera {
                session: live.clone(),
                camera: LiveCameraId::parse("camera-1").unwrap(),
            },
        ),
        (
            uris::STREAM_PRODUCTS_TEMPLATE,
            UavResource::StreamProducts(live.clone()),
        ),
        (
            uris::STREAM_PRODUCT_TEMPLATE,
            UavResource::StreamProduct {
                session: live.clone(),
                product: LiveStreamProductId::parse("product-1").unwrap(),
            },
        ),
        (
            uris::LIVE_VIEWS_TEMPLATE,
            UavResource::LiveViews {
                session: live.clone(),
                cursor: None,
            },
        ),
        (
            uris::LIVE_VIEWS_PAGE_TEMPLATE,
            UavResource::LiveViews {
                session: live.clone(),
                cursor: None,
            },
        ),
        (
            uris::LIVE_VIEW_TEMPLATE,
            UavResource::LiveView {
                session: live,
                view: LiveViewId::parse("view-1").unwrap(),
            },
        ),
        (
            uris::MISSION_TEMPLATE,
            UavResource::Mission(MissionId::parse("mission-1").unwrap()),
        ),
        (
            uris::CONTROL_GRANT_TEMPLATE,
            UavResource::ControlGrant(ControlGrantId::parse("grant-1").unwrap()),
        ),
        (
            uris::MISSION_PLAN_TEMPLATE,
            UavResource::MissionPlan(MissionPlanId::parse("plan-1").unwrap()),
        ),
        (
            uris::USAGE_TASK_TEMPLATE,
            UavResource::UsageTask("0195e2ec-54a1-7000-8000-000000000001".parse().unwrap()),
        ),
    ]
}

#[test]
fn checked_declaration_preserves_identity_capabilities_documents_and_scope_vocabulary() {
    let setup = McpServerSetup::<UavContract>::new().unwrap();
    let config = setup.server_config();
    assert_eq!(config.server_info.name, "uav-sim");
    assert_eq!(config.server_info.version, env!("CARGO_PKG_VERSION"));
    assert!(config.capabilities.tools.is_some());
    assert!(config.capabilities.prompts.is_some());
    assert!(config.capabilities.completions.is_some());
    let resources = config.capabilities.resources.as_ref().unwrap();
    assert_eq!(resources.subscribe, Some(true));
    assert_eq!(resources.list_changed, Some(true)); // Caller App metadata has a LIVE source.
    let extensions = config.capabilities.extensions.as_ref().unwrap();
    assert!(extensions.contains_key(rmcp::model::TASKS_EXTENSION_ID));
    assert!(extensions.contains_key(veoveo_mcp_apps_extension::EXTENSION_ID));
    assert_eq!(setup.documents().server(), "uav-sim");
    assert_eq!(
        setup.scope_names(),
        &UavScope::ALL
            .iter()
            .map(|scope| scope.name().clone())
            .collect()
    );
    let roots: BTreeSet<_> = setup
        .resources()
        .iter()
        .map(|resource| {
            assert_eq!(
                UavResource::parse(&resource.descriptor().uri).unwrap(),
                *resource.address()
            );
            resource.descriptor().uri.as_str()
        })
        .collect();
    assert_eq!(
        roots,
        BTreeSet::from([
            uris::DOCS,
            "uav-sim://docs/agents",
            "uav-sim://docs/design",
            uris::CONTRACT,
            uris::SESSIONS,
            uris::MISSIONS,
            uris::USAGE,
            uris::CONTROL_GRANTS,
            uris::MISSION_PLANS,
            uris::LIVE_APP_URI,
        ])
    );
}

#[test]
fn every_advertised_template_expands_to_its_domain_builder() {
    let cases = template_cases();
    assert_eq!(SERVER_SETUP.resource_templates().len(), cases.len());
    for (wire, expected) in cases {
        let template = SERVER_SETUP
            .resource_templates()
            .iter()
            .find(|template| template.template().as_str() == wire)
            .unwrap();
        assert_eq!(template.descriptor().uri_template, wire);
        let expanded = template.template().expand_scalars(&variables()).unwrap();
        assert_eq!(expanded, expected.to_uri().unwrap());
        assert_eq!(UavResource::parse(expanded.as_str()).unwrap(), expected);
        let mime = if wire == uris::DOC_TEMPLATE {
            "text/markdown"
        } else {
            "application/json"
        };
        assert_eq!(template.descriptor().mime_type.as_deref(), Some(mime));
        for variable in template.template().variables() {
            for bad in ["", "..", "a/b", "a?b", "a#b", "é"] {
                let mut context = variables();
                context.insert(variable.into(), bad.into());
                if let Ok(expanded) = template.template().expand_scalars(&context) {
                    assert!(
                        UavResource::parse(expanded.as_str()).is_err(),
                        "{wire}: {variable}={bad}"
                    );
                }
            }
        }
    }
}

#[test]
fn optional_cursor_templates_expand_the_exact_collection_bound_cursor() {
    let session = LiveSessionId::parse("Session_1.2").unwrap();
    let grant = UavGrantCursor::new(None, ControlGrantId::parse("grant-1").unwrap()).unwrap();
    let plan = UavPlanCursor::new(MissionPlanId::parse("plan-1").unwrap()).unwrap();
    let mission = UavMissionCursor::new(MissionId::parse("mission-1").unwrap()).unwrap();
    let live =
        UavLiveViewCursor::new(session.clone(), LiveViewId::parse("view-1").unwrap()).unwrap();
    let usage = UavUsageCursor::new(UavUsagePosition {
        created_at: "2026-09-28T12:00:00Z".parse().unwrap(),
        task_id: "0195e2ec-54a1-7000-8000-000000000001".parse().unwrap(),
    })
    .unwrap();
    for (wire, cursor, expected) in [
        (
            uris::CONTROL_GRANTS_PAGE_TEMPLATE,
            grant.as_str(),
            UavResource::ControlGrants {
                cursor: Some(grant.clone()),
            },
        ),
        (
            uris::MISSION_PLANS_PAGE_TEMPLATE,
            plan.as_str(),
            UavResource::MissionPlans {
                cursor: Some(plan.clone()),
            },
        ),
        (
            uris::MISSIONS_PAGE_TEMPLATE,
            mission.as_str(),
            UavResource::Missions {
                cursor: Some(mission.clone()),
            },
        ),
        (
            uris::USAGE_PAGE_TEMPLATE,
            usage.as_str(),
            UavResource::Usage {
                cursor: Some(usage.clone()),
            },
        ),
        (
            uris::LIVE_VIEWS_PAGE_TEMPLATE,
            live.as_str(),
            UavResource::LiveViews {
                session,
                cursor: Some(live.clone()),
            },
        ),
    ] {
        let mut variables = variables();
        variables.insert("cursor".into(), cursor.into());
        let template = SERVER_SETUP
            .resource_templates()
            .iter()
            .find(|template| template.template().as_str() == wire)
            .unwrap();
        let expanded = template.template().expand_scalars(&variables).unwrap();
        assert_eq!(expanded, expected.to_uri().unwrap());
        assert_eq!(UavResource::parse(expanded.as_str()).unwrap(), expected);
        variables.insert(
            "cursor".into(),
            UavMissionCursor::new(MissionId::parse("other").unwrap())
                .unwrap()
                .as_str()
                .into(),
        );
        if wire != uris::MISSIONS_PAGE_TEMPLATE {
            assert!(
                UavResource::parse(
                    template
                        .template()
                        .expand_scalars(&variables)
                        .unwrap()
                        .as_str()
                )
                .is_err()
            );
        }
    }
}

#[tokio::test]
async fn handlers_consume_checked_discovery_without_contacting_the_simulator() {
    use crate::{
        adapter::{Adapter, HttpAdapter},
        server::test_support,
    };
    use rmcp::{ServerHandler, service::serve_directly};
    use std::{sync::Arc, time::Duration};
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = test_support::fixture::TestDb::new().await;
    tokio::time::timeout(Duration::from_secs(60), async {
        let state = test_support::state(
            &db.a,
            Arc::new(Adapter::Http(Box::new(
                HttpAdapter::new(
                    "http://127.0.0.1:1/".parse().unwrap(),
                    Duration::from_secs(1),
                    Duration::from_secs(1),
                    "native-fixture".into(),
                    db.a.clone(),
                    "scope-test",
                )
                .unwrap(),
            ))),
            "setup-test",
        );
        let mut running = serve_directly(
            crate::server::service::hosted(state.clone()),
            (futures::sink::drain(), futures::stream::pending()),
            None,
        );
        let service = running.service();
        assert_eq!(
            serde_json::to_value(service.get_info()).unwrap(),
            serde_json::to_value(SERVER_SETUP.server_config()).unwrap()
        );
        for mask in 0..16 {
            let scopes: Vec<_> = UavScope::ALL
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .map(|(_, scope)| *scope)
                .collect();
            let context = test_support::context(running.peer(), &scopes, false);
            let result = service.list_resources(None, context.clone()).await;
            let can_read = scopes
                .iter()
                .any(|scope| matches!(scope, UavScope::Read | UavScope::Control | UavScope::Admin));
            if !can_read {
                assert!(result.is_err());
                continue;
            }
            let result = result.unwrap();
            let mut expected = BTreeSet::from([
                uris::DOCS,
                "uav-sim://docs/agents",
                "uav-sim://docs/design",
                uris::CONTRACT,
                uris::SESSIONS,
                uris::MISSIONS,
                uris::USAGE,
            ]);
            if scopes.contains(&UavScope::Control) || scopes.contains(&UavScope::Admin) {
                expected.extend([uris::CONTROL_GRANTS, uris::MISSION_PLANS]);
            }
            if scopes.contains(&UavScope::Stream) {
                expected.insert(uris::LIVE_APP_URI);
            }
            assert_eq!(
                result
                    .resources
                    .iter()
                    .map(|resource| resource.uri.as_str())
                    .collect::<BTreeSet<_>>(),
                expected,
                "scope mask {mask}"
            );
            assert!(
                result
                    .resources
                    .windows(2)
                    .all(|pair| pair[0].uri < pair[1].uri)
            );
            assert_eq!(result.cache_scope, Some(rmcp::model::CacheScope::Private));
            assert_eq!(
                result.ttl_ms,
                Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS)
            );
            assert!(result.next_cursor.is_none());
            if let Some(app) = result
                .resources
                .iter()
                .find(|resource| resource.uri == uris::LIVE_APP_URI)
            {
                let meta = veoveo_mcp_apps_extension::resource_ui_meta(app).unwrap();
                assert_eq!(
                    meta.csp.unwrap().connect_domains,
                    vec![state.live_view_connect_origin.clone()]
                );
                assert!(veoveo_mcp_apps_extension::resource_agent_message_targets(app).is_empty());
            }
            let result = service
                .list_resource_templates(None, context)
                .await
                .unwrap();
            assert_eq!(
                result.resource_templates,
                SERVER_SETUP
                    .resource_templates()
                    .iter()
                    .map(|template| template.descriptor().clone())
                    .collect::<Vec<_>>()
            );
            assert!(
                result
                    .resource_templates
                    .windows(2)
                    .all(|pair| pair[0].uri_template < pair[1].uri_template)
            );
            assert_eq!(result.cache_scope, Some(rmcp::model::CacheScope::Private));
            assert!(result.next_cursor.is_none());
        }
        running.close().await.unwrap();
    })
    .await
    .expect("checked UAV discovery exceeded 60 seconds");
}
