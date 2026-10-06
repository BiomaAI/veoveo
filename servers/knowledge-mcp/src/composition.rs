//! Installation-selected authority adapter and read-only prerequisite validation.
use std::sync::Arc;
use veoveo_modules::{
    CompositionIdentity, CredentialRevision, ExecutionCommand, ExecutionImage,
    InstallationGeneration, LaneExecution, ModuleName, ModulePlanDocument, ModuleRegistry,
};
use veoveo_platform_store::PlatformStore;
use veoveo_policy::internal_clients::{
    InternalClientAuthorityResolver, StaticInternalClientAuthorityResolver,
};

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
    pub async fn authority(
        &self,
        store: &PlatformStore,
    ) -> anyhow::Result<Arc<dyn InternalClientAuthorityResolver>> {
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
        let managed = self
            .plan
            .lanes()
            .iter()
            .any(|lane| lane.module.as_str() == "agents");
        let mut modules = veoveo_platform_store::schema::kernel_modules(execution)?;
        let mut required = vec![
            ModuleName::new("knowledge")?,
            ModuleName::new("gateway")?,
            ModuleName::new("identity")?,
        ];
        if let Some(module) = managed_module(managed)? {
            required.push(module.name().clone());
            modules.push(module);
        }
        let registry = ModuleRegistry::new(modules)?;
        veoveo_modules::runner::RuntimePrerequisites::new(&registry, self.plan, &required)?
            .require(
                store.client(),
                &veoveo_modules::preparation_key(self.plan, self.runtime_username)?,
                Default::default(),
            )
            .await?;
        if managed {
            #[cfg(feature = "managed-clients")]
            return Ok(Arc::new(
                veoveo_agent_runtime::internal_clients::ManagedInternalClientAuthorityResolver::new(
                    store.clone(),
                ),
            ));
        }
        Ok(Arc::new(StaticInternalClientAuthorityResolver))
    }
}

fn managed_module(selected: bool) -> anyhow::Result<Option<veoveo_modules::ModuleSetup>> {
    if !selected {
        return Ok(None);
    }
    #[cfg(feature = "managed-clients")]
    {
        Ok(Some(veoveo_agent_runtime::schema::module_setup(
            execution("agents")?,
        )?))
    }
    #[cfg(not(feature = "managed-clients"))]
    anyhow::bail!(
        "selected Agents lane requires the Knowledge managed-clients adapter; install a compatible Knowledge image"
    )
}
