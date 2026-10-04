/// Map-owned authorization vocabulary. External grants remain validated names.
/// ```compile_fail
/// use veoveo_map_mcp::contract::MapScope;
/// fn authorize(_: MapScope) {}
/// authorize("map:feature:read");
/// ```
/// ```compile_fail
/// use veoveo_map_mcp::contract::MapScope;
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
/// #[vocabulary(scope)]
/// enum OtherScope { #[vocabulary(rename = "other:read")]
/// Read
/// }
/// fn authorize(_: MapScope) {}
/// authorize(OtherScope::Read);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
pub enum MapScope {
    #[vocabulary(rename = "map:admin")]
    Admin,
    #[vocabulary(rename = "map:dataset:read")]
    DatasetRead,
    #[vocabulary(rename = "map:feature:read")]
    FeatureRead,
    #[vocabulary(rename = "map:feature:write")]
    FeatureWrite,
    #[vocabulary(rename = "map:feature:publish")]
    FeaturePublish,
    #[vocabulary(rename = "map:feature:admin")]
    FeatureAdmin,
    #[vocabulary(rename = "map:route")]
    Route,
    #[vocabulary(rename = "map:route_matrix")]
    RouteMatrix,
    #[vocabulary(rename = "map:spatial:derive")]
    SpatialDerive,
    #[vocabulary(rename = "map:raster:derive")]
    RasterDerive,
    #[vocabulary(rename = "map:restriction:publish")]
    RestrictionPublish,
    #[vocabulary(rename = "map:restriction:withdraw")]
    RestrictionWithdraw,
}
