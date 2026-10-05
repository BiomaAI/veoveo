use super::{ToolInputCase, assert_branches, qualify};

#[test]
fn computers_file_transfer_branches_are_admitted_and_closed() {
    use veoveo_computers_mcp::contract::TransferFileInput;
    let cases = ToolInputCase::load(include_bytes!(
        "../../../../../servers/computers-mcp/testdata/controlled-inputs.json"
    ));
    assert_branches(&cases, &["FileTransfer.import", "FileTransfer.export"]);
    for case in cases {
        assert_eq!(case.tool, "transfer_file");
        qualify::<TransferFileInput>(&case);
    }
}

#[test]
fn optimization_source_branches_are_admitted_and_closed() {
    use veoveo_optimization_mcp::contract::*;
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
        ],
    );
    for case in cases {
        match case.tool.as_str() {
            "solve_convex" => {
                qualify::<SolveConvexRequest>(&case);
                if let ConvexProblemSource::Inline { problem } =
                    case.decode::<SolveConvexRequest>().problem
                {
                    problem.validate().unwrap();
                }
            }
            "solve_milp" => {
                qualify::<SolveMilpRequest>(&case);
                if let MilpProblemSource::Inline { problem } =
                    case.decode::<SolveMilpRequest>().problem
                {
                    problem.validate().unwrap();
                }
            }
            "optimize_routes" => {
                qualify::<OptimizeRoutesRequest>(&case);
                if let RoutingProblemSource::Inline { problem } =
                    case.decode::<OptimizeRoutesRequest>().problem
                {
                    problem.validate().unwrap();
                }
            }
            _ => panic!("unexpected fixture tool"),
        }
    }
}

#[test]
fn map_controlled_branches_are_admitted_and_closed() {
    use veoveo_map_mcp::contract::*;
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
        ],
    );
    for case in cases {
        match case.tool.as_str() {
            "build_travel_model" => {
                qualify::<BuildTravelModelRequest>(&case);
                case.decode::<BuildTravelModelRequest>().validate().unwrap();
            }
            "query_source_features" => {
                qualify::<QuerySourceFeaturesRequest>(&case);
                case.decode::<QuerySourceFeaturesRequest>()
                    .validate()
                    .unwrap();
            }
            "derive_raster" => {
                qualify::<DeriveRasterRequest>(&case);
                case.decode::<DeriveRasterRequest>().validate().unwrap();
            }
            "register_mobility_profile" => {
                qualify::<CreateMobilityProfileRequest>(&case);
                case.decode::<CreateMobilityProfileRequest>()
                    .profile
                    .validate()
                    .unwrap();
            }
            "validate_feature_changes" => qualify::<ValidateFeatureChangesRequest>(&case),
            _ => panic!("unexpected fixture tool"),
        }
    }
}

#[test]
fn view_overlay_branches_are_admitted_and_closed() {
    use veoveo_view_mcp::contract::CreateSceneCompositionRequest;
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
        ],
    );
    for case in cases {
        assert_eq!(case.tool, "create_scene_composition");
        qualify::<CreateSceneCompositionRequest>(&case);
    }
}
