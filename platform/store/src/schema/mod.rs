//! Current kernel lane declarations; composition supplies their execution hosts.

pub mod artifacts;
pub mod audit;
pub mod gateway;
pub mod identity;
pub mod knowledge;
pub mod store;
pub mod tasks;

/// The complete always-selected kernel catalog, with execution supplied by composition.
pub fn kernel_modules(
    mut execution: impl FnMut(
        &str,
    )
        -> Result<veoveo_modules::LaneExecution, veoveo_modules::DeclarationError>,
) -> Result<Vec<veoveo_modules::ModuleSetup>, veoveo_modules::DeclarationError> {
    Ok(vec![
        store::module_setup(execution("store")?)?,
        identity::module_setup(execution("identity")?)?,
        gateway::module_setup(execution("gateway")?)?,
        artifacts::module_setup(execution("artifacts")?)?,
        tasks::module_setup(execution("tasks")?)?,
        audit::module_setup(execution("audit")?)?,
        knowledge::module_setup(execution("knowledge")?)?,
    ])
}
