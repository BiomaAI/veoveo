veoveo_types::scope_enum! {
    /// UAV-owned authorization vocabulary. A caller may also hold unrelated scopes.
    /// ```compile_fail
    /// use veoveo_uav_sim_mcp::contract::UavScope;
    /// fn authorize(_: UavScope) {}
    /// authorize("uav-sim:control");
    /// ```
    /// ```compile_fail
    /// use veoveo_uav_sim_mcp::contract::UavScope;
    /// fn authorize(_: UavScope) {}
    /// authorize(veoveo_types::ScopeName::new("uav-sim:control").unwrap());
    /// ```
    /// ```compile_fail
    /// use veoveo_uav_sim_mcp::contract::UavScope;
    /// veoveo_types::scope_enum! { enum OtherScope { Control => "other:control" } }
    /// fn authorize(_: UavScope) {}
    /// authorize(OtherScope::Control);
    /// ```
    pub enum UavScope {
        Read => "uav-sim:read",
        Control => "uav-sim:control",
        Admin => "uav-sim:admin",
        Stream => "uav-sim:stream",
    }
}
