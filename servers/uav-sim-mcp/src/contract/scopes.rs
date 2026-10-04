/// UAV-owned authorization vocabulary. A caller may also hold unrelated scopes.
/// ```compile_fail
/// use veoveo_uav_sim_mcp::contract::UavScope;
/// fn authorize(_: UavScope) {}
/// authorize("uav-sim:control");
/// ```
/// ```compile_fail
/// use veoveo_uav_sim_mcp::contract::UavScope;
/// fn authorize(_: UavScope) {}
/// authorize(veoveo_types::ScopeName::parse("uav-sim:control").unwrap());
/// ```
/// ```compile_fail
/// use veoveo_uav_sim_mcp::contract::UavScope;
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
/// #[vocabulary(scope)]
/// enum OtherScope { #[vocabulary(rename = "other:control")]
/// Control
/// }
/// fn authorize(_: UavScope) {}
/// authorize(OtherScope::Control);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
pub enum UavScope {
    #[vocabulary(rename = "uav-sim:read")]
    Read,
    #[vocabulary(rename = "uav-sim:control")]
    Control,
    #[vocabulary(rename = "uav-sim:admin")]
    Admin,
    #[vocabulary(rename = "uav-sim:stream")]
    Stream,
}
