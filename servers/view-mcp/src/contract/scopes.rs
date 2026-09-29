//! View owns the permission vocabulary; parsing a name establishes no grant.
veoveo_types::scope_enum! {
    /// View permissions remain separate capabilities in the authenticated grant set.
    /// ```compile_fail
    /// use veoveo_view_mcp::contract::ViewScope;
    /// fn requires_view(_: ViewScope) {}
    /// requires_view("view:read");
    /// ```
    pub enum ViewScope {
        Read => "view:read",
        Write => "view:write",
        Capture => "view:capture",
    }
}
