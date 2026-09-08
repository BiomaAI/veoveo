//! Compose qualified image evidence without rebuilding unrelated source artifacts.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use veoveo_deploy_contract::{
    DeploymentLock, ImageReleaseEvidence, LoadedProfile, LockedImage,
    components::{ComponentId, select_components},
};

use super::{absolute_output, profile_location, translate_registry, write_create_only_json};
use crate::{
    ReleaseImagesArgs,
    commands::{image, image_manifest},
    context::RepositoryContext,
};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceInput {
    source: String,
    path: PathBuf,
    digest: String,
}

#[derive(Debug)]
struct QualifiedInputs {
    images: BTreeMap<String, Vec<LockedImage>>,
    receipts: Vec<EvidenceInput>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicationReceipt<'a> {
    schema_version: &'static str,
    base_lock_digest: String,
    output_lock_digest: String,
    requested: &'a BTreeSet<ComponentId>,
    dependencies: Vec<ComponentId>,
    retained: Vec<ComponentId>,
    image_evidence: Vec<EvidenceInput>,
}

pub(super) fn publish(
    repository: &RepositoryContext,
    profile_path: &Path,
    args: &ReleaseImagesArgs,
) -> Result<()> {
    let _timing = image::operation::span(image::operation::Phase::Planning);
    let output = absolute_output(
        repository,
        args.lock_output
            .as_deref()
            .context("component publication requires --lock-output")?,
    );
    let receipt_path = output.with_file_name(format!(
        "{}.publication.json",
        output
            .file_name()
            .context("lock output has no filename")?
            .to_string_lossy()
    ));
    ensure!(
        !output.try_exists()? && !receipt_path.try_exists()?,
        "component publication outputs must be new files"
    );
    let base_path = absolute_output(
        repository,
        args.base_lock
            .as_deref()
            .context("component publication requires --base-lock")?,
    );
    let base_bytes = fs::read(&base_path).context("reading base deployment lock")?;
    let base: DeploymentLock = serde_json::from_slice(&base_bytes)?;
    base.validate()?;
    let mut requested = BTreeSet::new();
    for id in &args.component {
        ensure!(
            requested.insert(ComponentId::try_from(id.clone())?),
            "duplicate component selection {id}"
        );
    }
    let expanded = select_components(&base.components, &requested)?;
    let (profile_repository, profile_path, _) = profile_location(repository, profile_path)?;
    let profile = LoadedProfile::load(&profile_path, profile_repository.root())?;
    let QualifiedInputs {
        images,
        receipts: evidence_inputs,
    } = load_evidence(repository, &base, &args.image_evidence)?;
    // The runtime checks ownership and ignored evidence before opening source
    // snapshots. Registry qualification is read-only and precedes file publication.
    let updated =
        veoveo_deploy_runtime::update_component_images(&profile, &base, &requested, &images)?;
    for image in images.values().flatten() {
        let push = translate_registry(
            &image.repository,
            &base.registry.pull_address,
            &base.registry.push_address,
        )?;
        let inspected = image_manifest::inspect(
            &push,
            &image.publication_digest,
            "linux/amd64",
            base.registry.transport.is_insecure(),
        )?;
        ensure!(
            inspected.runtime == image.digest && inspected.publication == image.publication_digest,
            "qualified OCI publication differs from the supplied image evidence"
        );
    }
    let mut output_bytes = serde_json::to_vec_pretty(&updated)?;
    output_bytes.push(b'\n');
    let receipt = PublicationReceipt {
        schema_version: "veoveo.io/component-image-publication/v1",
        base_lock_digest: digest(&base_bytes),
        output_lock_digest: digest(&output_bytes),
        requested: &requested,
        dependencies: expanded
            .into_iter()
            .filter(|id| !requested.contains(id))
            .collect(),
        retained: base
            .components
            .iter()
            .map(|component| &component.declaration.id)
            .filter(|id| !requested.contains(id))
            .cloned()
            .collect(),
        image_evidence: evidence_inputs,
    };
    // Both files are create-only. The receipt records lock assembly, not cluster
    // execution; the existing command recorder includes total wall time and OCI reads.
    write_create_only_json(&receipt_path, &receipt)?;
    write_create_only_json(&output, &updated)?;
    println!("Deployment lock: {}", output.display());
    println!("Component publication receipt: {}", receipt_path.display());
    println!(
        "Rendered {} requested components; retained {} components.",
        requested.len(),
        receipt.retained.len()
    );
    Ok(())
}

fn load_evidence(
    repository: &RepositoryContext,
    base: &DeploymentLock,
    inputs: &[String],
) -> Result<QualifiedInputs> {
    ensure!(
        !inputs.is_empty(),
        "component publication requires --image-evidence SOURCE=PATH"
    );
    let mut images = BTreeMap::<String, Vec<LockedImage>>::new();
    let mut receipts = Vec::new();
    for input in inputs {
        let (source, path) = input
            .split_once('=')
            .context("image evidence must use SOURCE=PATH")?;
        ensure!(!path.is_empty(), "image evidence path is empty");
        ensure!(
            base.sources
                .iter()
                .any(|candidate| candidate.name == source),
            "image evidence source is outside the base lock"
        );
        let path = absolute_output(repository, Path::new(path));
        let bytes = fs::read(&path)
            .with_context(|| format!("reading qualified image evidence {}", path.display()))?;
        let evidence: ImageReleaseEvidence = serde_json::from_slice(&bytes)?;
        evidence.validate()?;
        ensure!(
            evidence.registry == base.registry,
            "image evidence registry differs from the base lock"
        );
        images
            .entry(source.to_owned())
            .or_default()
            .extend(evidence.images);
        receipts.push(EvidenceInput {
            source: source.to_owned(),
            path,
            digest: digest(&bytes),
        });
    }
    Ok(QualifiedInputs { images, receipts })
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn component_flags_require_a_complete_image_composition_and_reject_build_modes() {
        let args = [
            "xtask",
            "release",
            "images",
            "--profile",
            "deployment.json",
            "--base-lock",
            "base.json",
            "--component",
            "platform",
            "--image-evidence",
            "platform=evidence.json",
            "--lock-output",
            "new.json",
        ];
        assert!(crate::Cli::try_parse_from(args).is_ok());
        for flag in [
            "--base-lock",
            "--profile",
            "--component",
            "--image-evidence",
            "--lock-output",
        ] {
            let index = args.iter().position(|arg| *arg == flag).unwrap();
            let mut incomplete = args.to_vec();
            incomplete.drain(index..index + 2);
            assert!(
                crate::Cli::try_parse_from(incomplete).is_err(),
                "{flag} must be required"
            );
        }
        for flag in [
            "--profile-revision",
            "--revision",
            "--target",
            "--group",
            "--evidence-output",
            "--stage-evidence",
        ] {
            let mut conflicting = args.to_vec();
            conflicting.extend([flag, "value"]);
            assert!(
                crate::Cli::try_parse_from(conflicting).is_err(),
                "{flag} must conflict"
            );
        }
    }

    #[test]
    fn evidence_loading_rejects_staging_foreign_registry_and_unknown_source() {
        let repository =
            RepositoryContext::discover(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        let base: DeploymentLock = serde_json::from_str(include_str!(
            "../../../../../deploy/contract/tests/fixtures/deployment-lock.json"
        ))
        .unwrap();
        let source = &base.sources[0];
        let evidence = ImageReleaseEvidence {
            schema_version: veoveo_deploy_contract::IMAGE_RELEASE_EVIDENCE_SCHEMA.into(),
            source_revision: source.images[0].source_revision.clone(),
            registry: base.registry.clone(),
            images: vec![source.images[0].clone()],
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.json");
        let input = format!("{}={}", source.name, path.display());
        fs::write(&path, serde_json::to_vec(&evidence).unwrap()).unwrap();
        let QualifiedInputs { images, receipts } =
            load_evidence(&repository, &base, std::slice::from_ref(&input)).unwrap();
        assert_eq!(images[&source.name], evidence.images);
        assert_eq!(receipts[0].digest, digest(&fs::read(&path).unwrap()));
        assert!(
            load_evidence(&repository, &base, &[format!("unknown={}", path.display())]).is_err()
        );
        assert!(load_evidence(&repository, &base, &["missing-separator".into()]).is_err());
        let mut invalid = evidence.clone();
        invalid.schema_version = "veoveo.io/image-stage-evidence/v2".into();
        fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(load_evidence(&repository, &base, std::slice::from_ref(&input)).is_err());
        invalid = evidence;
        invalid.registry.push_address = "foreign.example.invalid".into();
        fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(
            load_evidence(&repository, &base, &[input])
                .unwrap_err()
                .to_string()
                .contains("registry differs")
        );
    }
}
