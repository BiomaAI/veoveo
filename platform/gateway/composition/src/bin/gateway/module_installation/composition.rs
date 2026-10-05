//! The binary composition owns real schema registrations and host associations.
use veoveo_modules::*;
pub(crate) fn registry() -> anyhow::Result<ModuleRegistry> {
    fn execution(name: &str) -> Result<LaneExecution, DeclarationError> {
        LaneExecution::new(
            ExecutionImage::new("gateway")?,
            ExecutionCommand::new(vec![
                "/usr/local/bin/gateway".into(),
                "module-migrate".into(),
                "--module".into(),
                name.into(),
            ])?,
        )
    }
    let mut modules = veoveo_platform_store::schema::kernel_modules(execution)?;
    modules.extend(vec![
        veoveo_computers::schema::module_setup(execution("computers")?)?,
        veoveo_agent_runtime::schema::module_setup(execution("agents")?)?,
        veoveo_workspace::schema::module_setup(execution("workspace")?)?,
        veoveo_recording_mcp::schema::module_setup(execution("recordings")?)?,
        veoveo_map_mcp::schema::module_setup(execution("map")?)?,
        veoveo_time_mcp::schema::module_setup(execution("time")?)?,
        veoveo_uav_sim_mcp::schema::module_setup(execution("uav")?)?,
        veoveo_frames_mcp::schema::module_setup(execution("frames")?)?,
        veoveo_media_mcp::schema::module_setup(execution("media")?)?,
        veoveo_optimization_mcp::schema::module_setup(execution("optimization")?)?,
    ]);
    Ok(ModuleRegistry::new(modules)?)
}
pub(super) fn bindings() -> anyhow::Result<Vec<ModuleRuntimeBinding>> {
    [
        ("agents", Some("agent-runtime-support"), None),
        ("recordings", Some("recording-data-plane"), None),
        ("recordings", None, Some("recording")),
        ("computers", None, Some("computers")),
        ("map", None, Some("map")),
        ("time", None, Some("time")),
        ("frames", None, Some("frames")),
        ("media", None, Some("media")),
        ("optimization", None, Some("optimization")),
    ]
    .into_iter()
    .map(|(module, component, mcp_server)| {
        Ok(ModuleRuntimeBinding {
            module: ModuleName::new(module)?,
            component: component.map(RuntimeBindingKey::new).transpose()?,
            mcp_server: mcp_server.map(RuntimeBindingKey::new).transpose()?,
        })
    })
    .collect()
}
pub(super) fn generate(
    selection: &ModuleSelectionDocument,
    composition: CompositionIdentity,
) -> anyhow::Result<ModulePlanDocument> {
    Ok(ModulePlanDocument::generate(
        &registry()?,
        selection,
        composition,
        bindings()?,
    )?)
}
