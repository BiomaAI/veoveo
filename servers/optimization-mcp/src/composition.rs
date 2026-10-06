//! Read-only installation prerequisites before Optimization Task recovery.
use veoveo_modules::{
    CompositionIdentity, CredentialRevision, ExecutionCommand, ExecutionImage,
    InstallationGeneration, LaneExecution, ModuleName, ModulePlanDocument, ModuleRegistry,
};
use veoveo_platform_store::PlatformStore;

/// Expected installation inputs are supplied by the same revisioned plan mount as
/// the Gateway; none selects a separate authority mode.
#[derive(Clone, Copy)]
pub struct RuntimeInstallation<'a> {
    pub plan: &'a ModulePlanDocument,
    pub composition: &'a CompositionIdentity,
    pub generation: InstallationGeneration,
    pub credential_revision: &'a CredentialRevision,
    pub runtime_username: &'a str,
}
fn execution(name: &str) -> Result<LaneExecution, veoveo_modules::DeclarationError> {
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
impl RuntimeInstallation<'_> {
    pub async fn require(&self, store: &PlatformStore) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.plan.composition() == self.composition,
            "module plan composition differs from expected locked image"
        );
        anyhow::ensure!(
            self.plan.generation() == self.generation,
            "module plan installation generation differs from expected generation"
        );
        anyhow::ensure!(
            self.plan.credential_revision() == self.credential_revision,
            "module plan credential revision differs from expected rotation"
        );
        anyhow::ensure!(
            store.config().username() == self.runtime_username,
            "database runtime username differs from installation preparation"
        );
        let mut modules = veoveo_platform_store::schema::kernel_modules(execution)?;
        modules.push(crate::schema::module_setup(execution("optimization")?)?);
        let required = vec![ModuleName::new("optimization")?];
        let registry = ModuleRegistry::new(modules)?;
        veoveo_modules::runner::RuntimePrerequisites::new(&registry, self.plan, &required)?
            .require(
                store.client(),
                &veoveo_modules::preparation_key(self.plan, self.runtime_username)?,
                Default::default(),
            )
            .await?;
        Ok(())
    }
}
