pub(super) fn config() -> rmcp::model::ClientConfig {
    use rmcp::model::{ClientCapabilities, ClientConfig, ElicitationCapability, Implementation};
    let mut capabilities = ClientCapabilities::default();
    capabilities
        .extensions
        .get_or_insert_default()
        .entry(rmcp::model::TASKS_EXTENSION_ID.into())
        .or_default();
    capabilities.elicitation = Some(
        ElicitationCapability::new()
            .with_form(Default::default())
            .with_url(Default::default()),
    );
    let (key, value) = veoveo_mcp_apps_extension::host_extension_capability();
    capabilities
        .extensions
        .get_or_insert_default()
        .insert(key, value);
    ClientConfig::new(
        capabilities,
        Implementation::new("veoveo-workspace", env!("CARGO_PKG_VERSION")),
    )
}
