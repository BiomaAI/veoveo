//! Actual pinned composition image and chart Job lifecycle in an owned namespace.
mod assertions;
mod fixture;
mod managed;
mod policy;
mod process;

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{fs, path::PathBuf, str::FromStr};
use veoveo_deploy_contract::ArtifactDigest;

#[derive(Clone, Debug)]
pub(crate) struct PinnedImage {
    repository: String,
    digest: ArtifactDigest,
}
impl FromStr for PinnedImage {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let reference: oci_spec::distribution::Reference =
            value.parse().map_err(|_| "OCI reference is invalid")?;
        if reference.tag().is_some() {
            return Err("image requires an untagged digest reference".into());
        }
        let digest = ArtifactDigest::parse(reference.digest().ok_or("image requires a digest")?)
            .map_err(|_| "image requires a lowercase SHA-256 digest")?;
        Ok(Self {
            repository: format!("{}/{}", reference.registry(), reference.repository()),
            digest,
        })
    }
}
impl PinnedImage {
    fn reference(&self) -> String {
        format!("{}@{}", self.repository, self.digest.as_str())
    }
}

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    #[arg(long)]
    pub context: String,
    #[arg(long)]
    pub gateway_image: PinnedImage,
    #[arg(long)]
    pub manager_image: PinnedImage,
    #[arg(long)]
    pub kernel_image: PinnedImage,
    #[arg(long)]
    pub evidence_output: PathBuf,
    /// Diagnostic only: validate generation-one rendered policies without installation.
    #[arg(long, requires = "policy_plan")]
    pub policy_server_dry_run: bool,
    #[arg(long, requires = "policy_server_dry_run")]
    pub policy_plan: Option<PathBuf>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    schema_version: &'static str,
    context: String,
    namespace: Option<String>,
    agent_namespace: Option<String>,
    image: String,
    manager_image: String,
    kernel_image: String,
    composition: String,
    cases: Vec<assertions::Case>,
    managed_recovery: Option<managed::Recovery>,
    failure: Option<String>,
    cleanup_failure: Option<String>,
}

pub(crate) fn verify(args: Args) -> Result<()> {
    if args.policy_server_dry_run {
        return policy::verify(&args);
    }
    ensure!(
        args.policy_plan.is_none(),
        "policy plan requires diagnostic-only mode"
    );
    ensure!(
        !args.evidence_output.exists(),
        "evidence output already exists"
    );
    let mut evidence = Evidence {
        schema_version: "veoveo.ai/module-installation-evidence/v1",
        context: args.context.clone(),
        namespace: None,
        agent_namespace: None,
        image: args.gateway_image.reference(),
        manager_image: args.manager_image.reference(),
        kernel_image: args.kernel_image.reference(),
        composition: args.gateway_image.digest.as_str().into(),
        cases: Vec::new(),
        managed_recovery: None,
        failure: None,
        cleanup_failure: None,
    };
    let mut fixture = None;
    let result = (|| {
        let owned = fixture::Fixture::create(&args)?;
        evidence.namespace = Some(owned.namespace.clone());
        evidence.agent_namespace = Some(owned.managed_config.namespace.clone());
        fixture = Some(owned);
        let owned = fixture.as_mut().expect("fixture initialized");
        owned.initialize().context("installation prerequisites")?;
        assertions::lifecycle(owned, &mut evidence.cases, &mut evidence.managed_recovery)
            .context("installed module lifecycle")
    })();
    if let Err(error) = &result {
        let diagnostics = fixture
            .as_ref()
            .and_then(|f| f.failure_diagnostics().ok())
            .unwrap_or_else(|| "fixture diagnostics unavailable".into());
        evidence.failure = Some(format!("{error:#}\n{diagnostics}"));
    }
    if let Some(owned) = fixture.as_mut()
        && let Err(error) = owned.cleanup()
    {
        evidence.cleanup_failure = Some(error.to_string());
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.evidence_output)?;
    serde_json::to_writer_pretty(&mut file, &evidence)?;
    file.sync_all()?;
    result?;
    ensure!(
        evidence.cleanup_failure.is_none(),
        "owned namespace cleanup failed"
    );
    println!(
        "Installed module lifecycle verified: {}",
        args.evidence_output.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_binding_is_digest_only() {
        assert!(
            format!(
                "registry.test:5000/veoveo/mcp-gateway@sha256:{}",
                "a".repeat(64)
            )
            .parse::<PinnedImage>()
            .is_ok()
        );
        for bad in [
            "gateway:latest",
            "gateway@sha256:bad",
            "gateway:latest@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ] {
            assert!(bad.parse::<PinnedImage>().is_err());
        }
    }
}
