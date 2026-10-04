//! Code-owned Task operations, independent of the database and MCP runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(task_type)]
pub enum MapTaskKind {
    #[vocabulary(rename = "route")]
    Route,
    #[vocabulary(rename = "route_matrix")]
    RouteMatrix,
    #[vocabulary(rename = "build_travel_model")]
    BuildTravelModel,
    #[vocabulary(rename = "reachable_area")]
    ReachableArea,
    #[vocabulary(rename = "inspect_geopackage")]
    InspectGeoPackage,
    #[vocabulary(rename = "import_feature_layer")]
    ImportFeatureLayer,
    #[vocabulary(rename = "export_feature_layer")]
    ExportFeatureLayer,
    #[vocabulary(rename = "build_vector_tiles")]
    BuildVectorTiles,
    #[vocabulary(rename = "derive_raster")]
    DeriveRaster,
}
