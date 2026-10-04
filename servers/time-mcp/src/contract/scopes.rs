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
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
/// #[vocabulary(scope)]
/// enum OtherScope { #[vocabulary(rename = "other:read")]
/// Read
/// }
/// fn requires_time(_: TimeScope) {}
/// requires_time(OtherScope::Read);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
pub enum TimeScope {
    #[vocabulary(rename = "time:read")]
    Read,
    #[vocabulary(rename = "time:schedule")]
    Schedule,
    #[vocabulary(rename = "time:timeline")]
    Timeline,
    #[vocabulary(rename = "time:event:write")]
    EventWrite,
    #[vocabulary(rename = "time:admin")]
    Admin,
}
