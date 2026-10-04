//! View owns the permission vocabulary; parsing a name establishes no grant.
/// View permissions remain separate capabilities in the authenticated grant set.
/// ```compile_fail
/// use veoveo_view_mcp::contract::ViewScope;
/// fn requires_view(_: ViewScope) {}
/// requires_view("view:read");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
pub enum ViewScope {
    #[vocabulary(rename = "view:read")]
    Read,
    #[vocabulary(rename = "view:write")]
    Write,
    #[vocabulary(rename = "view:capture")]
    Capture,
}
