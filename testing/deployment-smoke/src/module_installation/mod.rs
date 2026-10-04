//! Actual pinned composition image and chart Job lifecycle in an owned namespace.
mod assertions;
mod fixture;
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
        let reference: oci_spec::distribution::Reference = value
            .parse()
            .map_err(|_| "gateway OCI reference is invalid")?;
        if reference.tag().is_some() {
            return Err("gateway image requires an untagged digest reference".into());
        }
        let digest = ArtifactDigest::parse(
            reference
                .digest()
                .ok_or("gateway image requires a digest")?,
        )
        .map_err(|_| "gateway image requires a lowercase SHA-256 digest")?;
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
    pub evidence_output: PathBuf,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    schema_version: &'static str,
    context: String,
    namespace: Option<String>,
    image: String,
    composition: String,
    cases: Vec<assertions::Case>,
    failure: Option<String>,
    cleanup_failure: Option<String>,
}

pub(crate) fn verify(args: Args) -> Result<()> {
    ensure!(
        !args.evidence_output.exists(),
        "evidence output already exists"
    );
    let mut evidence = Evidence {
        schema_version: "veoveo.ai/module-installation-evidence/v1",
        context: args.context.clone(),
        namespace: None,
        image: args.gateway_image.reference(),
        composition: args.gateway_image.digest.as_str().into(),
        cases: Vec::new(),
        failure: None,
        cleanup_failure: None,
    };
    let mut fixture = None;
    let result = (|| {
        let owned = fixture::Fixture::create(&args)?;
        evidence.namespace = Some(owned.namespace.clone());
        fixture = Some(owned);
        let owned = fixture.as_mut().expect("fixture initialized");
        owned.initialize().context("installation prerequisites")?;
        assertions::lifecycle(owned, &mut evidence.cases).context("installed module lifecycle")
    })();
    if let Err(error) = &result {
        let diagnostics = fixture
            .as_ref()
            .and_then(|f| f.failure_diagnostics().ok())
            .unwrap_or_else(|| "fixture diagnostics unavailable".into());
        evidence.failure = Some(format!("{error:#}\n{diagnostics}"));
    }
    if let Some(owned) = fixture.as_mut()
        && owned.cleanup().is_err()
    {
        evidence.cleanup_failure = Some("owned namespace cleanup failed".into());
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
