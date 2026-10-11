//! Read-only installation admission before UAV plane startup effects.
use anyhow::{Context, Result, ensure};
use veoveo_modules::{
    CompositionIdentity, CredentialRevision, InstallationGeneration, LaneExecution, ModuleName,
    ModulePlanDocument, ModuleRegistry, PreparationKey, preparation_key,
    runner::{ExecutionLimits, RuntimePrerequisites},
};
use veoveo_platform_store::{PlatformStore, StoreAuthLevel, StoreConfig};

/// A composition supplies execution descriptors; the plane verifies compiled SQL
/// declarations and the full plan/account preparation, not image executability.
pub struct UavStartup {
    prerequisites: RuntimePrerequisites,
    preparation: PreparationKey,
    runtime_username: String,
}
impl UavStartup {
    pub fn from_env(store: &StoreConfig) -> Result<Self> {
        fn required(name: &str) -> Result<String> {
            std::env::var(name).with_context(|| format!("missing required env var {name}"))
        }
        let path = required("VEOVEO_MODULE_PLAN")?;
        let plan: ModulePlanDocument = serde_json::from_slice(
            &std::fs::read(&path).with_context(|| format!("reading module plan {path}"))?,
        )
        .context("admitting Uav installation plan")?;
        Self::new(
            &plan,
            CompositionIdentity::new(required("VEOVEO_MODULE_COMPOSITION")?)?,
            InstallationGeneration::new(required("VEOVEO_INSTALLATION_GENERATION")?.parse()?)?,
            CredentialRevision::new(required("VEOVEO_CREDENTIAL_REVISION")?)?,
            &required("VEOVEO_SURREAL_RUNTIME_USERNAME")?,
            store,
        )
    }
    pub fn new(
        plan: &ModulePlanDocument,
        composition: CompositionIdentity,
        generation: InstallationGeneration,
        credential_revision: CredentialRevision,
        runtime_username: &str,
        store: &StoreConfig,
    ) -> Result<Self> {
        ensure!(
            store.auth_level() == StoreAuthLevel::Database,
            "UAV startup requires database authentication"
        );
        ensure!(
            runtime_username == store.username(),
            "UAV runtime username differs from authenticated account"
        );
        ensure!(
            plan.composition() == &composition,
            "UAV plan composition differs from configured installation"
        );
        ensure!(
            plan.generation() == generation,
            "UAV plan generation differs from configured installation"
        );
        ensure!(
            plan.credential_revision() == &credential_revision,
            "UAV plan credential revision differs from configured installation"
        );
        let execution = |name: &str| -> Result<LaneExecution> {
            let lane = plan
                .lanes()
                .iter()
                .find(|lane| lane.module.as_str() == name)
                .with_context(|| format!("UAV installation plan lacks required lane {name}"))?;
            Ok(LaneExecution::new(
                lane.image.clone(),
                lane.command.clone(),
            )?)
        };
        use veoveo_platform_store::schema;
        let registry = ModuleRegistry::new(vec![
            schema::store::module_setup(execution("store")?)?,
            schema::identity::module_setup(execution("identity")?)?,
            schema::gateway::module_setup(execution("gateway")?)?,
            schema::artifacts::module_setup(execution("artifacts")?)?,
            schema::audit::module_setup(execution("audit")?)?,
            schema::tasks::module_setup(execution("tasks")?)?,
            veoveo_agent_runtime::schema::module_setup(execution("agents")?)?,
            crate::schema::module_setup(execution("uav")?)?,
        ])?;
        let roots = ["uav"]
            .into_iter()
            .map(ModuleName::new)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let prerequisites = RuntimePrerequisites::new(&registry, plan, &roots)?;
        let preparation = preparation_key(plan, runtime_username)?;
        Ok(Self {
            prerequisites,
            preparation,
            runtime_username: runtime_username.to_owned(),
        })
    }
    pub async fn require(&self, store: &PlatformStore) -> Result<()> {
        ensure!(
            store.config().auth_level() == StoreAuthLevel::Database
                && store.config().username() == self.runtime_username,
            "UAV connection differs from admitted runtime account"
        );
        self.prerequisites
            .require(
                store.client(),
                &self.preparation,
                ExecutionLimits::default(),
            )
            .await
            .context("UAV installation is not prepared for the compiled runtime lanes")
    }
}

#[cfg(test)]
#[path = "startup/tests.rs"]
mod tests;
