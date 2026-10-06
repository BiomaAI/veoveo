//! Hosted argument admission using isolated Store and DuckDB Spatial.
//! Valhalla and Python providers are unavailable; no routing or acquisition executes.
use super::*;
#[path = "../../../../testing/fixtures/tool_inputs.rs"]
mod input_fixture;
use serde_json::json;
use std::time::Duration;
use veoveo_mcp_contract::hosting::{
    Hosted,
    testing::{self, TestGateway},
};

#[tokio::test]
async fn unknown_tool_arguments_complete_before_map_operation() {
    let qualification = async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let extension = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION")
            .expect("hosted Map protocol test requires the qualified DuckDB Spatial extension");
        let db = crate::test_store::TestDb::with_modules(vec![
            crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap())
                .unwrap(),
        ])
        .await;
        let root = tempfile::tempdir().unwrap();
        let analytics =
            crate::analytics::MapAnalytics::open(crate::analytics::MapAnalyticsConfig {
                database_path: root.path().join("map.duckdb"),
                authoring_task_root: root.path().join("tasks"),
                spill_dir: root.path().join("spill"),
                spatial_extension: extension.into(),
                memory_limit: "64MB".into(),
                threads: 1,
            })
            .unwrap();
        let catalog = crate::catalog::MapCatalog::new(db.a.clone());
        let artifacts = crate::artifacts::ArtifactRepository::new("http://127.0.0.1:9");
        let client = crate::routes::valhalla::ValhallaClient::new(
            crate::routes::valhalla::ValhallaClientConfig {
                base_url: "http://127.0.0.1:9".parse().unwrap(),
                timeout: Duration::from_secs(1),
            },
        )
        .unwrap();
        let products = crate::release_products::ReleaseProducts::new(
            crate::release_products::ReleaseProductConfig {
                release_root: root.path().join("releases"),
                valhalla_active_dir: root.path().join("valhalla"),
                maximum_routing_expanded_bytes: 1024,
            },
            analytics.clone(),
        )
        .unwrap();
        let helper = crate::acquisition::AcquisitionHelper::new(
            crate::acquisition::AcquisitionHelperConfig {
                python_executable: root.path().join("unavailable-python"),
                module: "unavailable".into(),
                maximum_output_bytes: 1024,
            },
        )
        .unwrap();
        let acquisitions = crate::acquisition::AcquisitionService::new(
            crate::acquisition::AcquisitionServiceConfig {
                scratch_root: root.path().join("scratch"),
                mount_root: root.path().join("mount"),
                secret_root: root.path().join("secrets"),
                maximum_artifact_bytes: 1024,
            },
            catalog.clone(),
            helper,
            artifacts.clone(),
            products.clone(),
        )
        .unwrap();
        let state = Arc::new(crate::state::MapApplication {
            workspace_basemap: crate::contract::MapWorkspaceBasemap::open_free_map(
                "https://map.test/light.json",
                "https://map.test/dark.json",
            )
            .unwrap(),
            tasks: crate::task_lookup::bind(veoveo_task_runtime::TaskRuntime::new(
                db.a.clone(),
                "map",
                "strict-input",
            ))
            .unwrap(),
            catalog: catalog.clone(),
            analytics: analytics.clone(),
            authoring: crate::authoring::AuthoringService::new(db.a.clone(), analytics.clone()),
            routes: crate::routes::RouteService::new(
                catalog.clone(),
                analytics.clone(),
                crate::routes::valhalla::ValhallaPlanner::new(client.clone()),
            ),
            geography: crate::geography::GeographyService::new(catalog.clone(), analytics),
            raster: crate::raster::RasterService::new(crate::raster::RasterServiceConfig {
                python_executable: root.path().join("unavailable-python"),
                module: "unavailable".into(),
                maximum_output_bytes: 1024,
                timeout: Duration::from_secs(1),
            })
            .unwrap(),
            feature_packages: crate::feature_packages::FeaturePackageService::new(
                crate::feature_packages::FeaturePackageServiceConfig {
                    python_executable: root.path().join("unavailable-python"),
                    module: "unavailable".into(),
                    maximum_output_bytes: 1024,
                    timeout: Duration::from_secs(1),
                },
            )
            .unwrap(),
            spatial: crate::spatial::SpatialService::new(catalog),
            acquisitions: Arc::new(acquisitions),
            artifacts,
            products,
            valhalla_process: crate::routes::valhalla::ValhallaProcess::unavailable(
                root.path(),
                client,
            ),
            activation: Arc::new(tokio::sync::Mutex::new(())),
            subscriptions: Arc::new(veoveo_mcp_contract::SubscriptionHub::new()),
            authoring_task_root: root.path().join("tasks"),
            max_artifact_bytes: 1024,
        });
        assert!(state.valhalla_process.exited().await.unwrap());
        let app = veoveo_mcp_apps_extension::AppHtml::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/workspace-app.html"),
        )
        .unwrap();
        let handler = state.clone();
        // These scopes cover the direct and durable tools in this fixture inventory.
        // Invalid arguments must reach decoding rather than fail authorization first.
        let mut principal = testing::principal();
        principal.scopes.extend(
            [
                MapScope::Admin,
                MapScope::DatasetRead,
                MapScope::FeatureRead,
                MapScope::FeatureWrite,
                MapScope::FeaturePublish,
                MapScope::Route,
                MapScope::RouteMatrix,
                MapScope::SpatialDerive,
                MapScope::RasterDerive,
                MapScope::RestrictionPublish,
            ]
            .map(|scope| scope.name().clone()),
        );
        const KEY_ID: &str = "map-input-fixture";
        let token = veoveo_mcp_contract::GatewayInternalTokenIssuer::new(
            veoveo_types::TokenIssuer::parse(veoveo_mcp_contract::GATEWAY_INTERNAL_TOKEN_ISSUER)
                .unwrap(),
            testing::signing_key(KEY_ID),
        )
        .issue(
            veoveo_types::GatewayProfileId::parse("operations").unwrap(),
            veoveo_types::ServerSlug::parse("map").unwrap(),
            principal,
            testing::authority(),
            None,
            chrono::Utc::now() + chrono::TimeDelta::minutes(5),
        )
        .unwrap()
        .bearer_token;
        let gateway = TestGateway::new(
            testing::for_domain::<MapMcp>()
                .internal_trust(testing::trust_bundle(KEY_ID))
                .unwrap()
                .handler(move || Hosted::new(MapMcp::new(handler.clone(), app.clone())))
                .build(),
        );
        let discover = gateway
            .rpc_with("server/discover", json!({}), Some(&token))
            .await
            .1;
        assert!(discover.get("error").is_none(), "{discover}");
        let mut arguments = json!({"source_crs":"EPSG:4326","target_crs":"EPSG:4326","positions":[{"crs":"EPSG:4326","x":8.0,"y":47.0}]});
        let _: crate::contract::TransformCrsRequest =
            serde_json::from_value(arguments.clone()).unwrap();
        arguments["undeclared"] = true.into();
        let body = gateway
            .rpc_with(
                "tools/call",
                json!({"name":"transform_crs","arguments":arguments}),
                Some(&token),
            )
            .await
            .1;
        assert!(body.get("error").is_none(), "{body}");
        assert_eq!(body["result"]["resultType"], "complete", "{body}");
        let result: rmcp::model::CallToolResult =
            serde_json::from_value(body["result"].clone()).unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(
            serde_json::to_string(&result.content)
                .unwrap()
                .contains("undeclared")
        );

        let cases = input_fixture::ToolInputCase::load(include_bytes!(
            "../../testdata/controlled-inputs.json"
        ));
        assert_eq!(cases.len(), 240);
        for case in cases {
            match case.tool.as_str() {
                "build_travel_model" => {
                    let _: crate::contract::BuildTravelModelRequest = case.decode();
                }
                "query_source_features" => {
                    let _: crate::contract::QuerySourceFeaturesRequest = case.decode();
                }
                "derive_raster" => {
                    let _: crate::contract::DeriveRasterRequest = case.decode();
                }
                "register_mobility_profile" => {
                    let _: crate::contract::CreateMobilityProfileRequest = case.decode();
                }
                "validate_feature_changes" => {
                    let _: crate::contract::ValidateFeatureChangesRequest = case.decode();
                }
                "query_features" => {
                    let _: crate::contract::QueryFeaturesRequest = case.decode();
                }
                "route" => {
                    let _: crate::contract::RouteRequest = case.decode();
                }
                "reachable_area" => {
                    let _: crate::contract::ReachableAreaRequest = case.decode();
                }
                "import_feature_layer" => {
                    let _: crate::contract::ImportFeatureLayerRequest = case.decode();
                }
                "export_feature_layer" => {
                    let _: crate::contract::ExportFeatureLayerRequest = case.decode();
                }
                "derive_spatial_geometry" => {
                    let _: crate::contract::DeriveSpatialGeometryRequest = case.decode();
                }
                "register_source" => {
                    let _: crate::contract::CreateSourceRequest = case.decode();
                }
                "validate_geofence" => {
                    let _: crate::contract::ValidateGeofenceRequest = case.decode();
                }
                "create_feature_layer" => {
                    let _: crate::contract::CreateFeatureLayerRequest = case.decode();
                }
                "publish_restriction" => {
                    let _: crate::contract::PublishRestrictionRequest = case.decode();
                }
                "validate_route" => {
                    let _: crate::contract::ValidateRouteRequest = case.decode();
                }
                _ => panic!("unexpected fixture tool"),
            }
            for (location, arguments) in case
                .unknown_fields()
                .into_iter()
                .chain(case.invalid_values())
            {
                let body = gateway
                    .rpc_with(
                        "tools/call",
                        json!({"name":case.tool,"arguments":arguments}),
                        Some(&token),
                    )
                    .await
                    .1;
                assert!(
                    body.get("error").is_none(),
                    "{} {location}: {body}",
                    case.branch
                );
                assert_eq!(
                    body["result"]["resultType"], "complete",
                    "{} {location}: {body}",
                    case.branch
                );
                let result: rmcp::model::CallToolResult =
                    serde_json::from_value(body["result"].clone()).unwrap();
                assert_eq!(
                    result.is_error,
                    Some(true),
                    "{} {location}: {body}",
                    case.branch
                );
                case.assert_error(&location, &serde_json::to_string(&result.content).unwrap());
            }
        }
        assert!(state.tasks.list().await.unwrap().is_empty());
        let profiles: Vec<crate::persistence::MapMobilityProfileRecord> =
            db.a.client().select("map_mobility_profile").await.unwrap();
        assert!(profiles.is_empty());
        let restrictions: Vec<crate::persistence::MapRestrictionRecord> =
            db.a.client().select("map_restriction").await.unwrap();
        assert!(restrictions.is_empty());
        assert!(state.valhalla_process.exited().await.unwrap());
    };
    tokio::time::timeout(Duration::from_secs(120), qualification)
        .await
        .expect("Map argument admission exceeded 120 seconds");
}
