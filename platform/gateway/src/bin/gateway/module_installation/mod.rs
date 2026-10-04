//! Binary-owned composition and checked installation command inputs.
mod commands;
mod composition;
use anyhow::{Context, ensure};
use clap::Args;
pub(super) use commands::*;
use std::path::PathBuf;
use veoveo_modules::{
    CompositionIdentity, CredentialRevision, InstallationGeneration, ModulePlanDocument,
    ModuleSelectionDocument,
};

#[derive(Clone, Debug, Args)]
pub(super) struct PlanArgs {
    #[arg(long = "module-plan", env = "VEOVEO_MODULE_PLAN")]
    pub path: PathBuf,
    #[arg(long, env = "VEOVEO_MODULE_COMPOSITION")]
    pub composition: CompositionIdentity,
    #[arg(long, env = "VEOVEO_INSTALLATION_GENERATION")]
    pub generation: InstallationGeneration,
    #[arg(long, env = "VEOVEO_CREDENTIAL_REVISION")]
    pub credential_revision: CredentialRevision,
    #[arg(
        long = "surreal-runtime-username",
        env = "VEOVEO_SURREAL_RUNTIME_USERNAME"
    )]
    pub runtime_username: String,
}
impl PlanArgs {
    pub fn load(&self) -> anyhow::Result<ModulePlanDocument> {
        let supplied: ModulePlanDocument = serde_json::from_slice(
            &std::fs::read(&self.path).context("read generated module plan")?,
        )
        .context("decode generated module plan")?;
        ensure!(
            supplied.composition() == &self.composition,
            "module plan composition differs from expected locked image"
        );
        ensure!(
            supplied.generation() == self.generation,
            "module plan installation generation differs from expected generation"
        );
        ensure!(
            supplied.credential_revision() == &self.credential_revision,
            "module plan credential revision differs from expected rotation"
        );
        let regenerated = composition::generate(&supplied.selection()?, self.composition.clone())?;
        ensure!(
            supplied == regenerated,
            "module plan differs from this binary's compiled composition; regenerate with the locked gateway image"
        );
        // Admission is deliberately before callers connect or provision any database resources.
        let registry = composition::registry()?;
        veoveo_modules::runner::prepare(registry.select(supplied.enabled().to_vec())?)?;
        Ok(supplied)
    }
}
pub(super) fn generate(
    path: &PathBuf,
    composition: CompositionIdentity,
    helm_values: bool,
) -> anyhow::Result<()> {
    let selection: ModuleSelectionDocument =
        serde_json::from_slice(&std::fs::read(path).context("read module selection")?)
            .context("decode module selection")?;
    let plan = composition::generate(&selection, composition)?;
    let json = serde_json::to_string(&plan)?;
    if helm_values {
        println!(
            "{}",
            serde_json::json!({"moduleInstallation":{"planJson":json}})
        );
    } else {
        println!("{json}");
    }
    Ok(())
}

pub(super) fn preparation_key(
    args: &PlanArgs,
    plan: &ModulePlanDocument,
) -> anyhow::Result<veoveo_modules::PreparationKey> {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for bytes in [
        b"veoveo.ai/installation-preparation/v1".as_slice(),
        serde_json::to_vec(plan)?.as_slice(),
        args.runtime_username.as_bytes(),
        veoveo_platform_store::schema_catalog_identity().as_bytes(),
    ] {
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    let digest: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(veoveo_modules::PreparationKey::new(
        plan.generation(),
        digest,
    )?)
}
