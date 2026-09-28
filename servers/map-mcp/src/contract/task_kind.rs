//! Code-owned Task operations, independent of the database and MCP runtime.
veoveo_types::declare_task_types! {
    pub enum MapTaskKind {
        Route => "route",
        RouteMatrix => "route_matrix",
        BuildTravelModel => "build_travel_model",
        ReachableArea => "reachable_area",
        InspectGeoPackage => "inspect_geopackage",
        ImportFeatureLayer => "import_feature_layer",
        ExportFeatureLayer => "export_feature_layer",
        BuildVectorTiles => "build_vector_tiles",
        DeriveRaster => "derive_raster",
    }
}
