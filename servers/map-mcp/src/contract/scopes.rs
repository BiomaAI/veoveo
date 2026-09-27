veoveo_types::scope_enum! {
    /// Map-owned authorization vocabulary. External grants remain validated names.
    /// ```compile_fail
    /// use veoveo_map_mcp::contract::MapScope;
    /// fn authorize(_: MapScope) {}
    /// authorize("map:feature:read");
    /// ```
    /// ```compile_fail
    /// use veoveo_map_mcp::contract::MapScope;
    /// veoveo_types::scope_enum! { enum OtherScope { Read => "other:read" } }
    /// fn authorize(_: MapScope) {}
    /// authorize(OtherScope::Read);
    /// ```
    pub enum MapScope {
        Admin => "map:admin",
        DatasetRead => "map:dataset:read",
        FeatureRead => "map:feature:read",
        FeatureWrite => "map:feature:write",
        FeaturePublish => "map:feature:publish",
        FeatureAdmin => "map:feature:admin",
        Route => "map:route",
        RouteMatrix => "map:route_matrix",
        SpatialDerive => "map:spatial:derive",
        RasterDerive => "map:raster:derive",
        RestrictionPublish => "map:restriction:publish",
        RestrictionWithdraw => "map:restriction:withdraw",
    }
}
