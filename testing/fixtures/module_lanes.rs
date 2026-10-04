//! Apply actual owner lanes after the shared fixture's explicit mixed-schema preparation.
use veoveo_modules::*;
use veoveo_platform_store::PlatformStore;

pub fn execution(name: &str) -> Result<LaneExecution, DeclarationError> {
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
pub async fn install(
    store: &PlatformStore,
    optional: Vec<ModuleSetup>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let selected = optional.iter().map(|m| m.name().clone()).collect();
    let mut owners = vec![
        veoveo_platform_store::schema::store::module_setup(execution("store")?)?,
        veoveo_platform_store::schema::identity::module_setup(execution("identity")?)?,
        veoveo_platform_store::schema::gateway::module_setup(execution("gateway")?)?,
        veoveo_platform_store::schema::artifacts::module_setup(execution("artifacts")?)?,
        veoveo_platform_store::schema::tasks::module_setup(execution("tasks")?)?,
        veoveo_platform_store::schema::audit::module_setup(execution("audit")?)?,
        veoveo_platform_store::schema::knowledge::module_setup(execution("knowledge")?)?,
    ];
    owners.extend(optional);
    let registry = ModuleRegistry::new(owners)?;
    veoveo_modules::runner::prepare(registry.select(selected)?)?
        .apply(store.client())
        .await?;
    Ok(())
}
