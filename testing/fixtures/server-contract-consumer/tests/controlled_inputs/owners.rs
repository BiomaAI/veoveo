//! Finite inventory of controlled variants reachable from the 114 production input roots.
//! URI-string dispatch, provider maps, response and admin-only vocabularies do not
//! invent object branches. Their owning address/admission suites qualify them.
use super::{ToolInputCase, assert_branches, qualify};

#[test]
fn artifact_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/artifact-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(
        &cases,
        &[
            "AccessSubject.principal",
            "AccessSubject.group",
            "AccessLevel.read",
            "AccessLevel.write",
            "AccessLevel.admin",
            "ArtifactReleaseState.private",
            "ArtifactReleaseState.releasable",
            "ArtifactReleaseState.released",
        ],
    );
    for case in cases {
        match case.tool.as_str() {
            "grant_access" => qualify::<veoveo_artifact_mcp::contract::GrantArtifactRequest>(&case),
            "set_release_state" => {
                qualify::<veoveo_artifact_mcp::contract::SetArtifactReleaseRequest>(&case)
            }
            _ => panic!("unexpected fixture tool"),
        }
    }
}

#[test]
fn computers_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/computers-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(
        &cases,
        &[
            "FileTransfer.import",
            "FileTransfer.export",
            "AutomationPermission.read",
            "AutomationPermission.execute",
            "AutomationPermission.start",
            "AutomationPermission.stop",
            "AutomationInterruption.stop_computer",
        ],
    );
    for case in cases {
        match case.tool.as_str() {
            "transfer_file" => qualify::<veoveo_computers_mcp::contract::TransferFileInput>(&case),
            "grant_automation" => {
                qualify::<veoveo_computers_mcp::contract::IssueAutomationGrantInput>(&case)
            }
            _ => panic!("unexpected fixture tool"),
        }
    }
}

#[test]
fn duckdb_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/duckdb-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(
        &cases,
        &[
            "DuckDbSource.inline_csv",
            "DuckDbSource.uri",
            "DuckDbSource.uris",
            "DuckDbSource.artifact",
            "DuckDbIngestMode.create",
            "DuckDbIngestMode.append",
            "DuckDbIngestMode.replace",
            "DuckDbFormat.auto",
            "DuckDbFormat.csv",
            "DuckDbFormat.parquet",
            "DuckDbFormat.json",
            "DuckDbFormat.ndjson",
            "DuckDbReadOptionValue.bool",
            "DuckDbReadOptionValue.number",
            "DuckDbReadOptionValue.string",
            "DuckDbReadOptionValue.array",
            "DuckDbQueryOutputMode.inline",
            "DuckDbQueryOutputMode.artifact",
            "DuckDbTabularFormat.parquet",
            "DuckDbTabularFormat.csv",
            "DuckDbExportFormat.parquet",
            "DuckDbExportFormat.csv",
            "DuckDbExportFormat.duck_db",
            "DuckDbTabularSelection.table",
            "DuckDbTabularSelection.sql",
            "SnapshotSelection.database",
        ],
    );
    for case in cases {
        match case.tool.as_str() {
            "ingest" => qualify::<veoveo_duckdb_mcp::contract::DuckDbIngestRequest>(&case),
            "query" => qualify::<veoveo_duckdb_mcp::contract::DuckDbQueryRequest>(&case),
            "export" => qualify::<veoveo_duckdb_mcp::contract::DuckDbExportRequest>(&case),
            _ => panic!("unexpected fixture tool"),
        }
    }
}

#[test]
fn frames_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/frames-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(
        &cases,
        &[
            "CoordinatePoint.wgs84",
            "CoordinateSpace.wgs84",
            "CoordinatePoint.ecef_wgs84",
            "CoordinateSpace.ecef_wgs84",
            "CoordinatePoint.world_frame",
            "CoordinateSpace.world_frame",
            "FrameBasis.ecef_wgs84",
            "FrameBasis.enu",
            "FrameBasis.ned",
            "FrameBasis.frd",
            "FrameBasis.optical_rdf",
            "FrameBasis.cartesian",
            "FrameParentTransform.geodetic_tangent",
            "FrameParentTransform.static_rigid",
            "FrameParentTransform.dynamic_stream",
            "FrameAxisDirection.right",
            "FrameAxisDirection.left",
            "FrameAxisDirection.up",
            "FrameAxisDirection.down",
            "FrameAxisDirection.forward",
            "FrameAxisDirection.back",
        ],
    );
    for case in cases {
        match case.tool.as_str() {
            "convert_frame" => qualify::<veoveo_frames_mcp::contract::ConvertFrameRequest>(&case),
            "publish_world" => qualify::<veoveo_frames_mcp::contract::PublishWorldRequest>(&case),
            _ => panic!("unexpected fixture tool"),
        }
    }
}

#[test]
fn knowledge_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/knowledge-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(&cases, &["EmbedRequest.document", "EmbedRequest.query"]);
    for case in cases {
        match case.tool.as_str() {
            "embed" => qualify::<veoveo_knowledge_mcp::contract::EmbedRequest>(&case),
            _ => panic!("unexpected fixture tool"),
        }
    }
}

#[test]
fn map_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/map-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(
        &cases,
        &[
            "TravelTimeModel.static",
            "TravelTimeModel.invariant_local_departure",
            "SourceSpatialQuery.bounding_box",
            "SourceSpatialQuery.intersects",
            "SourceSpatialQuery.contains",
            "SourceSpatialQuery.within",
            "SourceSpatialQuery.within_distance",
            "SourceSpatialQuery.nearest",
            "RasterDerivationOperation.sample",
            "RasterDerivationOperation.window",
            "RasterDerivationOperation.class_mask",
            "RasterDerivationOperation.contour",
            "RasterDerivationOperation.polygonize",
            "RasterDerivationOperation.skeletonize",
            "RasterDerivationOperation.derive_lines",
            "RasterDerivationOperation.corridor_maximum",
            "MobilityProfile.human",
            "MobilityProfile.road_vehicle",
            "MobilityProfile.off_road_vehicle",
            "MobilityProfile.rail_vehicle",
            "MobilityProfile.surface_vessel",
            "MobilityProfile.subsurface_vessel",
            "MobilityProfile.fixed_wing",
            "MobilityProfile.rotorcraft",
            "MobilityProfile.uas",
            "FeatureMutation.create",
            "FeatureMutation.replace",
            "FeatureMutation.tombstone",
            "FeatureMutation.restore",
            "Cql2Expression.operation",
            "Cql2Expression.property",
            "Cql2Expression.literal",
            "Cql2Literal.string",
            "Cql2Literal.number",
            "Cql2Literal.boolean",
            "Cql2Literal.null",
            "Cql2Operator.and",
            "Cql2Operator.or",
            "Cql2Operator.not",
            "Cql2Operator.=",
            "Cql2Operator.<>",
            "Cql2Operator.<",
            "Cql2Operator.<=",
            "Cql2Operator.>",
            "Cql2Operator.>=",
            "Cql2Operator.isNull",
            "RouteEndpoint.position",
            "RouteEndpoint.location",
            "RouteEndpoint.facility",
            "RouteObjectiveKind.fastest",
            "RouteObjectiveKind.shortest",
            "RouteObjectiveKind.lowest_energy",
            "RouteObjectiveKind.lowest_risk",
            "RouteObjectiveKind.lowest_cost",
            "RouteObjectiveKind.weighted",
            "ReachableBudget.duration",
            "ReachableBudget.distance",
            "FeatureImportSource.geo_json_feature_collection",
            "FeatureImportSource.geo_json_text_sequence",
            "FeatureImportSource.geo_package",
            "FeatureExportFormat.geo_json_seq",
            "FeatureExportFormat.geo_parquet",
            "FeatureExportFormat.geo_package",
            "SpatialDerivationOperation.resample_line",
            "SpatialDerivationOperation.order_points",
            "SpatialDerivationOperation.polygon_boundary",
            "SpatialDerivationOperation.standoff_perimeter",
            "SpatialDerivationOperation.corridor",
            "SpatialDerivationOperation.parallel_lanes",
            "SpatialDerivationOperation.racetrack",
            "SpatialDerivationOperation.stations",
            "SpatialDerivationOperation.coverage",
            "SpatialDerivationOperation.connected_components",
            "SpatialDerivationOperation.ingress",
            "SpatialDerivationOperation.validate_route",
            "StandoffSide.inward",
            "StandoffSide.outward",
            "TurnDirection.clockwise",
            "TurnDirection.counter_clockwise",
            "StationKind.relay",
            "StationKind.station",
            "SourceAdapterKind.open_street_map",
            "SourceAdapterKind.authority_vector",
            "SourceAdapterKind.gtfs_schedule",
            "SourceAdapterKind.gtfs_realtime",
            "SourceAdapterKind.s57_enc",
            "SourceAdapterKind.s100",
            "SourceAdapterKind.aixm",
            "SourceAdapterKind.faa_nasr",
            "SourceAdapterKind.environmental",
            "AuthorityClass.regulator",
            "AuthorityClass.infrastructure_operator",
            "AuthorityClass.hydrographic_office",
            "AuthorityClass.air_navigation_service_provider",
            "AuthorityClass.transport_operator",
            "AuthorityClass.commercial_provider",
            "AuthorityClass.community",
            "AuthorityClass.derived",
            "AuthorityClass.synthetic_test",
            "AcquisitionModel.snapshot",
            "AcquisitionModel.sequenced_delta",
            "AcquisitionModel.effective_event",
            "AcquisitionModel.observation_stream",
            "MapFamily.road_street",
            "MapFamily.active_mobility",
            "MapFamily.rail_transit",
            "MapFamily.off_road_terrain",
            "MapFamily.maritime",
            "MapFamily.aviation",
            "MapFamily.intermodal",
            "SourceLocation.https",
            "SourceLocation.osm_replication",
            "SourceLocation.mounted_exchange_set",
            "SourceCredential.bearer",
            "SourceCredential.header",
            "FeatureGeometry.Point",
            "FeatureGeometry.MultiPoint",
            "FeatureGeometry.LineString",
            "FeatureGeometry.MultiLineString",
            "FeatureGeometry.Polygon",
            "FeatureGeometry.MultiPolygon",
            "FeatureGeometryType.Point",
            "FeatureGeometryType.MultiPoint",
            "FeatureGeometryType.LineString",
            "FeatureGeometryType.MultiLineString",
            "FeatureGeometryType.Polygon",
            "FeatureGeometryType.MultiPolygon",
            "HumanMovementMode.walk",
            "HumanMovementMode.run",
            "HumanMovementMode.hike",
            "HumanMovementMode.manual_mobility_aid",
            "HumanMovementMode.powered_mobility_aid",
            "RoadVehicleClass.bicycle",
            "RoadVehicleClass.powered_two_wheeler",
            "RoadVehicleClass.passenger_car",
            "RoadVehicleClass.light_commercial",
            "RoadVehicleClass.rigid_truck",
            "RoadVehicleClass.articulated_truck",
            "RoadVehicleClass.bus_coach",
            "RoadVehicleClass.emergency_service",
            "OffRoadLocomotionClass.wheeled",
            "OffRoadLocomotionClass.tracked",
            "OffRoadLocomotionClass.atv_utv",
            "OffRoadLocomotionClass.heavy_equipment",
            "OffRoadLocomotionClass.uncrewed_ground_vehicle",
            "RailVehicleClass.light_rail_metro",
            "RailVehicleClass.passenger_train",
            "RailVehicleClass.freight_train",
            "RailVehicleClass.maintenance_train",
            "SurfaceVesselClass.small_craft",
            "SurfaceVesselClass.cargo",
            "SurfaceVesselClass.tanker",
            "SurfaceVesselClass.passenger_ferry",
            "SurfaceVesselClass.tug_workboat",
            "SurfaceVesselClass.fishing_service",
            "SurfaceVesselClass.uncrewed_surface_vessel",
            "SubsurfaceVesselClass.submarine",
            "SubsurfaceVesselClass.autonomous_underwater_vehicle",
            "SubsurfaceVesselClass.remotely_operated_vehicle",
            "SubsurfaceVesselClass.underwater_glider",
            "FixedWingClass.light",
            "FixedWingClass.regional_transport",
            "FixedWingClass.heavy_cargo",
            "FixedWingClass.amphibious",
            "RotorcraftClass.helicopter",
            "RotorcraftClass.heavy_lift_helicopter",
            "RotorcraftClass.tiltrotor",
            "UasClass.multirotor",
            "UasClass.fixed_wing",
            "UasClass.hybrid_vtol",
            "EnergySource.human",
            "EnergySource.battery",
            "EnergySource.gasoline",
            "EnergySource.diesel",
            "EnergySource.aviation_fuel",
            "EnergySource.hydrogen",
            "EnergySource.natural_gas",
            "EnergySource.nuclear",
            "EnergySource.wind",
            "EnergySource.hybrid",
            "EnergySource.external_electric",
            "EnergySource.other",
            "TravelCostMetric.duration",
            "TravelCostMetric.distance",
            "GeofenceRule.must_remain_inside",
            "GeofenceRule.must_remain_outside",
            "GeofenceRule.must_not_cross_boundary",
            "FeatureContentClass.reference",
            "FeatureContentClass.named_locations",
            "FeatureContentClass.facilities",
            "FeatureContentClass.boundaries",
            "FeatureContentClass.network_candidate",
            "SourceFeatureRepresentation.point",
            "SourceFeatureRepresentation.line",
            "SourceFeatureRepresentation.polygon",
            "SourceFeatureRepresentation.relation",
            "RestrictionKind.closure",
            "RestrictionKind.access",
            "RestrictionKind.dimensional_limit",
            "RestrictionKind.weight_limit",
            "RestrictionKind.hazardous_cargo",
            "RestrictionKind.speed_limit",
            "RestrictionKind.environmental",
            "RestrictionKind.protected_area",
            "RestrictionKind.navigational_warning",
            "RestrictionKind.airspace",
            "RestrictionKind.weather",
            "RestrictionKind.other",
            "RestrictionEffectKind.prohibit",
            "RestrictionEffectKind.require",
            "RestrictionEffectKind.limit",
            "RestrictionEffectKind.penalize",
            "RestrictionEffectKind.advise",
            "MobilityFamily.human",
            "MobilityFamily.road_vehicle",
            "MobilityFamily.off_road_vehicle",
            "MobilityFamily.rail_vehicle",
            "MobilityFamily.surface_vessel",
            "MobilityFamily.subsurface_vessel",
            "MobilityFamily.fixed_wing",
            "MobilityFamily.rotorcraft",
            "MobilityFamily.uas",
            "RestrictionLimit.maximum_height",
            "RestrictionLimit.maximum_width",
            "RestrictionLimit.maximum_length",
            "RestrictionLimit.maximum_mass",
            "RestrictionLimit.maximum_speed",
            "RestrictionLimit.minimum_depth",
            "RestrictionLimit.minimum_altitude",
            "RestrictionLimit.maximum_altitude",
            "RestrictionLimit.minimum_reserve",
            "VerticalReference.ellipsoid",
            "VerticalReference.mean_sea_level",
            "VerticalReference.above_ground_level",
            "VerticalReference.chart_datum",
            "RouteStatus.planning_advisory",
            "RouteStatus.validated",
            "RouteStatus.stale",
            "RouteStatus.invalidated",
            "RouteStatus.unavailable",
        ],
    );
    for case in cases {
        match case.tool.as_str() {
            "build_travel_model" => {
                qualify::<veoveo_map_mcp::contract::BuildTravelModelRequest>(&case)
            }
            "query_source_features" => {
                qualify::<veoveo_map_mcp::contract::QuerySourceFeaturesRequest>(&case)
            }
            "derive_raster" => qualify::<veoveo_map_mcp::contract::DeriveRasterRequest>(&case),
            "register_mobility_profile" => {
                qualify::<veoveo_map_mcp::contract::CreateMobilityProfileRequest>(&case)
            }
            "validate_feature_changes" => {
                qualify::<veoveo_map_mcp::contract::ValidateFeatureChangesRequest>(&case)
            }
            "query_features" => qualify::<veoveo_map_mcp::contract::QueryFeaturesRequest>(&case),
            "route" => qualify::<veoveo_map_mcp::contract::RouteRequest>(&case),
            "reachable_area" => qualify::<veoveo_map_mcp::contract::ReachableAreaRequest>(&case),
            "import_feature_layer" => {
                qualify::<veoveo_map_mcp::contract::ImportFeatureLayerRequest>(&case)
            }
            "export_feature_layer" => {
                qualify::<veoveo_map_mcp::contract::ExportFeatureLayerRequest>(&case)
            }
            "derive_spatial_geometry" => {
                qualify::<veoveo_map_mcp::contract::DeriveSpatialGeometryRequest>(&case)
            }
            "register_source" => qualify::<veoveo_map_mcp::contract::CreateSourceRequest>(&case),
            "validate_geofence" => {
                qualify::<veoveo_map_mcp::contract::ValidateGeofenceRequest>(&case)
            }
            "create_feature_layer" => {
                qualify::<veoveo_map_mcp::contract::CreateFeatureLayerRequest>(&case)
            }
            "publish_restriction" => {
                qualify::<veoveo_map_mcp::contract::PublishRestrictionRequest>(&case)
            }
            "validate_route" => qualify::<veoveo_map_mcp::contract::ValidateRouteRequest>(&case),
            _ => panic!("unexpected fixture tool"),
        }
        use veoveo_map_mcp::contract::*;
        if case.branch.starts_with("Cql2Expression.") || case.branch.starts_with("Cql2Literal.") {
            let input: QueryFeaturesRequest = case.decode();
            let filter = input.filter.unwrap();
            let value = &filter.args[usize::from(case.branch.starts_with("Cql2Literal."))];
            assert!(
                match case.branch.as_str() {
                    "Cql2Expression.operation" => matches!(value, Cql2Expression::Operation(_)),
                    "Cql2Expression.property" => matches!(value, Cql2Expression::Property(_)),
                    "Cql2Expression.literal" | "Cql2Literal.string" =>
                        matches!(value, Cql2Expression::Literal(Cql2Literal::String(_))),
                    "Cql2Literal.number" =>
                        matches!(value, Cql2Expression::Literal(Cql2Literal::Number(_))),
                    "Cql2Literal.boolean" =>
                        matches!(value, Cql2Expression::Literal(Cql2Literal::Boolean(_))),
                    "Cql2Literal.null" =>
                        matches!(value, Cql2Expression::Literal(Cql2Literal::Null(_))),
                    _ => unreachable!(),
                },
                "{} decoded wrong owner variant",
                case.branch
            );
        }
        match case.branch.split_once('.').unwrap().0 {
            "TravelTimeModel" => case.decode::<BuildTravelModelRequest>().validate().unwrap(),
            "SourceSpatialQuery" => case
                .decode::<QuerySourceFeaturesRequest>()
                .validate()
                .unwrap(),
            "RasterDerivationOperation" => case.decode::<DeriveRasterRequest>().validate().unwrap(),
            "MobilityProfile" => case
                .decode::<CreateMobilityProfileRequest>()
                .profile
                .validate()
                .unwrap(),
            "SpatialDerivationOperation" => case
                .decode::<DeriveSpatialGeometryRequest>()
                .validate()
                .unwrap(),
            _ => {}
        }
    }
}

#[test]
fn media_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/media-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(&cases, &["RunArgs.object"]);
    for case in cases {
        match case.tool.as_str() {
            "run" => qualify::<veoveo_media_mcp::contract::RunArgs>(&case),
            _ => panic!("unexpected fixture tool"),
        }
    }
}

#[test]
fn optimization_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/optimization-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(
        &cases,
        &[
            "ConvexProblemSource.inline",
            "ConvexProblemSource.resource",
            "ConvexProblemSource.artifact",
            "MilpProblemSource.inline",
            "MilpProblemSource.resource",
            "MilpProblemSource.artifact",
            "RoutingProblemSource.inline",
            "RoutingProblemSource.resource",
            "RoutingProblemSource.artifact",
            "TravelModelSource.map_resource",
            "TravelModelSource.artifact",
            "TravelModelSource.inline",
            "ConvexProblemKind.linear_program",
            "ConvexProblemKind.quadratic_program",
            "ConvexProblemKind.quadratically_constrained_program",
            "ConvexProblemKind.second_order_cone_program",
            "VariableKind.continuous",
            "VariableKind.integer",
            "VariableKind.semi_continuous",
            "ObjectiveDirection.minimize",
            "ObjectiveDirection.maximize",
            "TimeUnit.second",
            "TimeUnit.minute",
            "RouteOrderKind.service",
            "RouteOrderKind.pickup_delivery",
            "RouteServicePolicy.mandatory",
            "RouteServicePolicy.optional",
            "RouteObjectiveMetric.cost",
            "RouteObjectiveMetric.travel_time",
            "RouteObjectiveMetric.route_size_variance",
            "RouteObjectiveMetric.route_service_time_variance",
            "RouteObjectiveMetric.prize",
            "RouteObjectiveMetric.vehicle_fixed_cost",
            "ArtifactModelFormat.optimization_json_v2",
        ],
    );
    // Admit the whole positive corpus before exercising every schema/default/negative control.
    // A failed baseline must not hide later owner relationship failures in this family.
    let baseline_failures: Vec<_> = cases
        .iter()
        .filter_map(|case| {
            let bytes = serde_json::to_vec(&case.arguments).unwrap();
            let admission = match case.tool.as_str() {
                "solve_convex" => serde_json::from_slice::<
                    veoveo_optimization_mcp::contract::SolveConvexRequest,
                >(&bytes)
                .map(|_| ()),
                "solve_milp" => serde_json::from_slice::<
                    veoveo_optimization_mcp::contract::SolveMilpRequest,
                >(&bytes)
                .map(|_| ()),
                "optimize_routes" => serde_json::from_slice::<
                    veoveo_optimization_mcp::contract::OptimizeRoutesRequest,
                >(&bytes)
                .map(|_| ()),
                _ => panic!("unexpected fixture tool"),
            };
            admission
                .err()
                .map(|error| format!("{} baseline: {error}", case.branch))
        })
        .collect();
    assert!(
        baseline_failures.is_empty(),
        "Optimization baseline admission failures:\n{}",
        baseline_failures.join("\n")
    );
    for case in cases {
        match case.tool.as_str() {
            "solve_convex" => {
                qualify::<veoveo_optimization_mcp::contract::SolveConvexRequest>(&case)
            }
            "solve_milp" => qualify::<veoveo_optimization_mcp::contract::SolveMilpRequest>(&case),
            "optimize_routes" => {
                qualify::<veoveo_optimization_mcp::contract::OptimizeRoutesRequest>(&case)
            }
            _ => panic!("unexpected fixture tool"),
        }
        use veoveo_optimization_mcp::contract::*;
        match case.branch.as_str() {
            "ConvexProblemSource.inline" => {
                let ConvexProblemSource::Inline { problem } =
                    case.decode::<SolveConvexRequest>().problem
                else {
                    unreachable!()
                };
                problem.validate().unwrap();
            }
            "MilpProblemSource.inline" => {
                let MilpProblemSource::Inline { problem } =
                    case.decode::<SolveMilpRequest>().problem
                else {
                    unreachable!()
                };
                problem.validate().unwrap();
            }
            "RoutingProblemSource.inline" => {
                let RoutingProblemSource::Inline { problem } =
                    case.decode::<OptimizeRoutesRequest>().problem
                else {
                    unreachable!()
                };
                problem.validate().unwrap();
            }
            _ => {}
        }
    }
}

#[test]
fn reason_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/reason-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(
        &cases,
        &[
            "ReasoningTask.describe_segment",
            "ReasoningTask.detect_events",
            "ReasoningTask.answer_question",
            "DecodePolicy.greedy",
            "DecodePolicy.sampled",
        ],
    );
    for case in cases {
        match case.tool.as_str() {
            "analyze_recording" => {
                qualify::<veoveo_reason_mcp::contract::AnalyzeRecordingRequest>(&case)
            }
            _ => panic!("unexpected fixture tool"),
        }
    }
}

#[test]
fn recording_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/recording-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(
        &cases,
        &[
            "RecordingProjectionSampling.range",
            "RecordingProjectionSampling.latest_at",
            "RecordingProjectionSampling.sample_grid",
            "RecordingProjectionSparseFill.none",
            "RecordingProjectionSparseFill.latest_at_global",
        ],
    );
    for case in cases {
        match case.tool.as_str() {
            "create_recording_projection" => {
                qualify::<veoveo_recording_mcp::contract::CreateRecordingProjectionRequest>(&case)
            }
            _ => panic!("unexpected fixture tool"),
        }
    }
}

#[test]
fn speech_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/speech-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(
        &cases,
        &["TranscribeRequest.object", "StartDictation.object"],
    );
    for case in cases {
        match case.tool.as_str() {
            "transcribe" => qualify::<veoveo_speech_mcp::contract::TranscribeRequest>(&case),
            "start_dictation" => {
                qualify::<veoveo_speech_mcp::contract::dictation::StartDictation>(&case)
            }
            _ => panic!("unexpected fixture tool"),
        }
    }
}

#[test]
fn stream_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/stream-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(
        &cases,
        &[
            "SamplingPolicy.every_frame",
            "SamplingPolicy.every_nth",
            "SamplingPolicy.maximum_frames",
        ],
    );
    for case in cases {
        match case.tool.as_str() {
            "run_recording" => qualify::<veoveo_stream_mcp::contract::RunRecordingRequest>(&case),
            _ => panic!("unexpected fixture tool"),
        }
    }
}

#[test]
fn time_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/time-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(
        &cases,
        &[
            "TimeExpression.rfc3339",
            "TimeExpression.rfc9557",
            "TimeExpression.civil",
            "TimeExpression.unix",
            "TimeExpression.tai",
            "TimeExpression.gps",
            "TimeExpression.julian_tai",
            "TimeExpression.military_dtg",
            "TimeExpression.epoch_relative",
            "Disambiguation.reject",
            "Disambiguation.earlier",
            "Disambiguation.later",
            "TimeScale.utc",
            "TimeScale.tai",
            "TimeScale.tt",
            "TimeScale.tdb",
            "TimeScale.gpst",
            "TimeScale.gst",
            "WindowOperation.union",
            "WindowOperation.intersection",
            "WindowOperation.difference",
            "RecurrenceFrequency.daily",
            "RecurrenceFrequency.weekly",
            "Weekday.monday",
            "Weekday.tuesday",
            "Weekday.wednesday",
            "Weekday.thursday",
            "Weekday.friday",
            "Weekday.saturday",
            "Weekday.sunday",
        ],
    );
    for case in cases {
        match case.tool.as_str() {
            "resolve_time" => qualify::<veoveo_time_mcp::contract::ResolveTimeRequest>(&case),
            "convert_time" => qualify::<veoveo_time_mcp::contract::ConvertTimeRequest>(&case),
            "evaluate_windows" => {
                qualify::<veoveo_time_mcp::contract::EvaluateWindowsRequest>(&case)
            }
            "expand_schedule" => qualify::<veoveo_time_mcp::contract::ExpandScheduleRequest>(&case),
            _ => panic!("unexpected fixture tool"),
        }
    }
}

#[test]
fn timeseries_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/timeseries-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(
        &cases,
        &[
            "TimeseriesFilterPredicate.eq",
            "TimeseriesFilterPredicate.ne",
            "TimeseriesFilterPredicate.in",
            "TimeseriesFilterPredicate.is_not_null",
            "TimeseriesFilterValue.string",
            "TimeseriesFilterValue.bool",
            "TimeseriesFilterValue.i64",
            "TimeseriesFilterValue.u64",
            "TimeseriesFilterValue.f64",
            "DuckDbTabularSource.inline_csv",
            "DuckDbTabularSource.uri",
            "DuckDbTabularSource.uris",
            "DuckDbFormat.auto",
            "DuckDbFormat.csv",
            "DuckDbFormat.parquet",
            "DuckDbFormat.json",
            "DuckDbFormat.ndjson",
            "TimeseriesFilterCombination.all",
            "TimeseriesFilterCombination.any",
            "TimeseriesForecastMethod.naive_trend",
            "DuckDbReadOptionValue.bool",
            "DuckDbReadOptionValue.number",
            "DuckDbReadOptionValue.string",
            "DuckDbReadOptionValue.array",
        ],
    );
    for case in cases {
        match case.tool.as_str() {
            "forecast" => {
                qualify::<veoveo_timeseries_mcp::contract::TimeseriesForecastRequest>(&case)
            }
            _ => panic!("unexpected fixture tool"),
        }
        if case.branch.starts_with("TimeseriesFilterValue.") {
            use veoveo_timeseries_mcp::contract::{
                TimeseriesFilterPredicate, TimeseriesFilterValue, TimeseriesForecastRequest,
            };
            let input: TimeseriesForecastRequest = case.decode();
            let filter = input.training_filter.unwrap();
            let TimeseriesFilterPredicate::Eq { value, .. } = &filter.predicates()[0] else {
                panic!("filter value fixture must use Eq");
            };
            assert!(
                match case.branch.as_str() {
                    "TimeseriesFilterValue.string" =>
                        matches!(value, TimeseriesFilterValue::String(_)),
                    "TimeseriesFilterValue.bool" => matches!(value, TimeseriesFilterValue::Bool(_)),
                    "TimeseriesFilterValue.i64" => matches!(value, TimeseriesFilterValue::I64(_)),
                    "TimeseriesFilterValue.u64" => matches!(value, TimeseriesFilterValue::U64(_)),
                    "TimeseriesFilterValue.f64" => matches!(value, TimeseriesFilterValue::F64(_)),
                    _ => unreachable!(),
                },
                "{} decoded wrong owner variant",
                case.branch
            );
        }
    }
}

#[test]
fn uav_sim_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/uav-sim-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(
        &cases,
        &[
            "VehicleControlPermission.inspect",
            "VehicleControlPermission.plan",
            "VehicleControlPermission.execute",
            "VehicleControlPermission.abort",
            "SessionRequest.object",
            "FrameBasis.ecef_wgs84",
            "FrameBasis.enu",
            "FrameBasis.ned",
            "FrameBasis.frd",
            "FrameBasis.optical_rdf",
            "FrameBasis.cartesian",
            "FrameParentTransform.geodetic_tangent",
            "FrameParentTransform.static_rigid",
            "FrameParentTransform.dynamic_stream",
            "FrameAxisDirection.right",
            "FrameAxisDirection.left",
            "FrameAxisDirection.up",
            "FrameAxisDirection.down",
            "FrameAxisDirection.forward",
            "FrameAxisDirection.back",
            "MapRouteHandoffSchema.veoveo.ai/map-route-handoff/v2",
            "RouteStatus.validated",
            "RouteStatus.planning_advisory",
        ],
    );
    for case in cases {
        match case.tool.as_str() {
            "grant_vehicle_control" => {
                qualify::<veoveo_uav_sim_mcp::contract::GrantVehicleControlRequest>(&case)
            }
            "get_simulation_state" => {
                qualify::<veoveo_uav_sim_mcp::contract::SessionRequest>(&case)
            }
            "configure_world" => {
                qualify::<veoveo_uav_sim_mcp::contract::ConfigureWorldRequest>(&case)
            }
            "prepare_vehicle_mission" => {
                qualify::<veoveo_uav_sim_mcp::contract::PrepareVehicleMissionRequest>(&case)
            }
            _ => panic!("unexpected fixture tool"),
        }
    }
}

#[test]
fn view_controlled_input_branches_are_admitted_and_closed() {
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/view-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(
        &cases,
        &[
            "ScenePosition.wgs84",
            "ScenePosition.local_meters",
            "SceneOverlayGeometry.marker",
            "SceneOverlayGeometry.polyline",
            "SceneOverlayGeometry.polygon",
            "SceneOverlayGeometry.oriented_mesh_instance",
            "SceneOverlayGeometry.label",
            "SceneOverlayGeometrySource.inline",
            "SceneOverlayGeometrySource.artifact",
            "CameraDefinition.pose",
            "CameraDefinition.look_at",
            "CameraDefinition.orbit_target",
            "DeadlineBehavior.return_best_available",
            "DeadlineBehavior.fail",
            "FrameEncoding.png",
            "FrameEncoding.jpeg",
        ],
    );
    for case in cases {
        match case.tool.as_str() {
            "create_scene_composition" => {
                qualify::<veoveo_view_mcp::contract::CreateSceneCompositionRequest>(&case)
            }
            "set_camera" => qualify::<veoveo_view_mcp::contract::SetCameraRequest>(&case),
            "capture_frame" => qualify::<veoveo_view_mcp::contract::CaptureFrameRequest>(&case),
            _ => panic!("unexpected fixture tool"),
        }
    }
}
