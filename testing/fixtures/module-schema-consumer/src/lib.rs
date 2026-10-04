//! Independent composition consumer: schema-only owners, with no runtime feature unification.
use veoveo_modules::{DeclarationError, LaneExecution, ModuleSetup};

/// Compose owner declarations; the caller supplies its reviewed execution host per lane.
pub fn declarations(
    mut execution: impl FnMut(&str) -> Result<LaneExecution, DeclarationError>,
) -> Result<Vec<ModuleSetup>, DeclarationError> {
    Ok(vec![
        veoveo_platform_store::schema::store::module_setup(execution("store")?)?,
        veoveo_platform_store::schema::identity::module_setup(execution("identity")?)?,
        veoveo_platform_store::schema::gateway::module_setup(execution("gateway")?)?,
        veoveo_platform_store::schema::artifacts::module_setup(execution("artifacts")?)?,
        veoveo_platform_store::schema::tasks::module_setup(execution("tasks")?)?,
        veoveo_platform_store::schema::audit::module_setup(execution("audit")?)?,
        veoveo_platform_store::schema::knowledge::module_setup(execution("knowledge")?)?,
        veoveo_computers::schema::module_setup(execution("computers")?)?,
        veoveo_agent_runtime::schema::module_setup(execution("agents")?)?,
        veoveo_workspace::schema::module_setup(execution("workspace")?)?,
        veoveo_recording_mcp::schema::module_setup(execution("recordings")?)?,
        veoveo_map_mcp::schema::module_setup(execution("map")?)?,
        veoveo_time_mcp::schema::module_setup(execution("time")?)?,
        veoveo_uav_sim_mcp::schema::module_setup(execution("uav")?)?,
        veoveo_frames_mcp::schema::module_setup(execution("frames")?)?,
        veoveo_media_mcp::schema::module_setup(execution("media")?)?,
    ])
}

#[cfg(test)]
mod tests;
