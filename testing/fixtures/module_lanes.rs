//! Compose and apply current kernel and explicitly selected owner lanes.
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
pub fn registry(optional: Vec<ModuleSetup>) -> Result<ModuleRegistry, DeclarationError> {
    let mut owners = veoveo_platform_store::schema::kernel_modules(execution)?;
    owners.extend(optional);
    ModuleRegistry::new(owners)
}

#[allow(
    dead_code,
    reason = "Direct native fixtures reuse lane installation without the container helper"
)]
pub async fn install(
    store: &PlatformStore,
    optional: Vec<ModuleSetup>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let selected = optional.iter().map(|m| m.name().clone()).collect();
    let registry = registry(optional)?;
    veoveo_modules::runner::prepare(registry.select(selected)?)?
        .apply(store.client())
        .await?;
    Ok(())
}
