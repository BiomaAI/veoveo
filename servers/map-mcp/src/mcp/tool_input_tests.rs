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

fn fixture_field<T: serde::de::DeserializeOwned>(wire: &serde_json::Value, key: &str) -> T {
    serde_json::from_value(wire[key].clone()).unwrap()
}

/// Rebuild controlled owner inputs from admitted fields; Serde owns their wire spelling.
fn current_fixture_arguments(case: &serde_json::Value) -> Option<serde_json::Value> {
    use crate::contract::*;
    let arguments = &case["arguments"];
    match case["tool"].as_str().unwrap() {
        "register_source" => {
            let source = &arguments["source"];
            let source = RegisteredSource::new(RegisteredSourceValue {
                source_id: fixture_field(source, "sourceId"),
                dataset_id: fixture_field(source, "datasetId"),
                name: fixture_field(source, "name"),
                adapter_kind: fixture_field(source, "adapterKind"),
                authority: fixture_field(source, "authority"),
                acquisition_model: fixture_field(source, "acquisitionModel"),
                map_families: fixture_field(source, "mapFamilies"),
                location: fixture_field(source, "location"),
                credential: fixture_field(source, "credential"),
                publisher_key_refs: source
                    .get("publisherKeyRefs")
                    .map(|value| serde_json::from_value(value.clone()).unwrap())
                    .unwrap_or_default(),
                expected_media_types: fixture_field(source, "expectedMediaTypes"),
                maximum_download_bytes: fixture_field(source, "maximumDownloadBytes"),
                maximum_elapsed_seconds: fixture_field(source, "maximumElapsedSeconds"),
                license: fixture_field(source, "license"),
                enabled: fixture_field(source, "enabled"),
                record_version: 1,
                created_at: fixture_field(source, "createdAt"),
                updated_at: fixture_field(source, "updatedAt"),
            })
            .unwrap();
            Some(
                serde_json::to_value(CreateSourceRequest {
                    source,
                    idempotency_key: fixture_field(arguments, "idempotencyKey"),
                })
                .unwrap(),
            )
        }
        "publish_restriction" => {
            let restriction = &arguments["restriction"];
            let restriction = Restriction::new(RestrictionValue {
                restriction_id: fixture_field(restriction, "restrictionId"),
                kind: fixture_field(restriction, "kind"),
                geometry: fixture_field(restriction, "geometry"),
                vertical_band: fixture_field(restriction, "verticalBand"),
                affected_mobility_families: fixture_field(restriction, "affectedMobilityFamilies"),
                effect: fixture_field(restriction, "effect"),
                valid_from: fixture_field(restriction, "validFrom"),
                valid_until: fixture_field(restriction, "validUntil"),
                authority: fixture_field(restriction, "authority"),
                source_release_id: fixture_field(restriction, "sourceReleaseId"),
                issued_at: fixture_field(restriction, "issuedAt"),
                cancelled_by: fixture_field(restriction, "cancelledBy"),
                record_version: 1,
            })
            .unwrap();
            Some(serde_json::to_value(PublishRestrictionRequest { restriction }).unwrap())
        }
        _ => None,
    }
}

#[test]
fn actual_controlled_input_producer_matches_current_fixture() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/controlled-inputs.json");
    let captured: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut current = captured.clone();
    let mut generated = 0;
    for case in current.as_array_mut().unwrap() {
        if let Some(arguments) = current_fixture_arguments(case) {
            case["arguments"] = arguments;
            generated += 1;
        }
    }
    assert_eq!(generated, 73);
    if std::env::var_os("UPDATE_MAP_INPUT_FIXTURES").is_some() {
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&current).unwrap() + "\n",
        )
        .unwrap();
    }
    let restored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(current, restored);
    for case in input_fixture::ToolInputCase::load(&std::fs::read(path).unwrap()) {
        match case.tool.as_str() {
            "register_source" => {
                let _: crate::contract::CreateSourceRequest = case.decode();
            }
            "publish_restriction" => {
                let _: crate::contract::PublishRestrictionRequest = case.decode();
            }
            _ => {}
        }
    }
}

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
        let mut arguments = json!({"sourceCrs":"EPSG:4326","targetCrs":"EPSG:4326","positions":[{"crs":"EPSG:4326","x":8.0,"y":47.0}]});
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
