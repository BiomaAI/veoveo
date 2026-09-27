veoveo_types::scope_enum! {
    /// Scopes understood by Time. Other servers' scopes stay in the grant set.
    ///
    /// ```compile_fail
    /// use veoveo_time_mcp::contract::TimeScope;
    /// fn requires_time(_: TimeScope) {}
    /// requires_time("time:read");
    /// ```
    /// ```compile_fail
    /// use veoveo_time_mcp::contract::TimeScope;
    /// use veoveo_types::ScopeName;
    /// fn requires_time(_: TimeScope) {}
    /// requires_time(ScopeName::new("time:read").unwrap());
    /// ```
    /// ```compile_fail
    /// use veoveo_time_mcp::contract::TimeScope;
    /// veoveo_types::scope_enum! { enum OtherScope { Read => "other:read" } }
    /// fn requires_time(_: TimeScope) {}
    /// requires_time(OtherScope::Read);
    /// ```
    pub enum TimeScope {
        Read => "time:read",
        Schedule => "time:schedule",
        Timeline => "time:timeline",
        EventWrite => "time:event:write",
        Admin => "time:admin",
    }
}
