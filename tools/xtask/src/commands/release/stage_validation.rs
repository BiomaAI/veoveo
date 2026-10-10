//! Admit an entire original staging receipt before comparing selected qualified images.
use super::{IMAGE_STAGE_EVIDENCE_SCHEMA, ImageStageEvidenceInput, validate_oci_digest};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;
use veoveo_deploy_contract::{LockedImage, LockedRegistry};

pub(super) fn validate(
    staged: ImageStageEvidenceInput,
    revision: &str,
    registry: &LockedRegistry,
    qualified: &[LockedImage],
) -> Result<()> {
    ensure!(
        staged.schema_version == IMAGE_STAGE_EVIDENCE_SCHEMA,
        "staged image evidence uses unsupported schema {}",
        staged.schema_version
    );
    ensure!(
        !staged.release_eligible,
        "staged image evidence must declare releaseEligible=false"
    );
    ensure!(
        staged.source_revision == revision,
        "staged source revision {} does not match qualified revision {revision}",
        staged.source_revision
    );
    ensure!(
        &staged.registry == registry,
        "staged registry endpoints do not match qualified registry endpoints"
    );
    ensure!(
        !qualified.is_empty(),
        "staged qualification requires a nonempty selection"
    );
    let mut admitted = BTreeMap::new();
    for image in staged.images {
        ensure!(
            image.platform == "linux/amd64",
            "staged image {} uses unsupported platform {}",
            image.target,
            image.platform
        );
        validate_oci_digest(&image.runtime_digest, "staged runtime")?;
        validate_oci_digest(&image.staging_index_digest, "staging index")?;
        ensure!(
            admitted.insert(image.target.clone(), image).is_none(),
            "staged evidence repeats an image target"
        );
    }
    for image in qualified {
        let prior = admitted
            .get(&image.name)
            .with_context(|| format!("staged evidence omits qualified image {}", image.name))?;
        ensure!(
            prior.repository == image.repository,
            "qualified image {} changed repository from {} to {}",
            image.name,
            prior.repository,
            image.repository
        );
        ensure!(
            prior.runtime_digest == image.digest,
            "qualified image {} changed runnable digest from {} to {}; qualification may attach attestations but must not rebuild runtime identity",
            image.name,
            prior.runtime_digest,
            image.digest
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::release::StagedImageInput;
    use veoveo_deploy_contract::{RegistryTransport, SourceRevision};

    const REVISION: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    fn registry() -> LockedRegistry {
        LockedRegistry {
            push_address: "registry.example.invalid".into(),
            pull_address: "registry.example.invalid".into(),
            transport: RegistryTransport::Tls,
        }
    }
    fn digest(hex: char) -> String {
        format!("sha256:{}", hex.to_string().repeat(64))
    }
    fn row(target: &str) -> StagedImageInput {
        StagedImageInput {
            target: target.into(),
            repository: format!("registry.example.invalid/owner/{target}"),
            runtime_digest: digest('b'),
            staging_index_digest: digest('c'),
            platform: "linux/amd64".into(),
        }
    }
    fn stage() -> ImageStageEvidenceInput {
        ImageStageEvidenceInput {
            schema_version: IMAGE_STAGE_EVIDENCE_SCHEMA.into(),
            source_revision: REVISION.into(),
            registry: registry(),
            release_eligible: false,
            images: vec![row("selected"), row("unselected")],
        }
    }
    fn qualified(target: &str) -> LockedImage {
        let staged = row(target);
        LockedImage {
            name: staged.target,
            repository: staged.repository,
            source_revision: SourceRevision::parse(REVISION).unwrap(),
            digest: staged.runtime_digest,
            publication_digest: digest('d'),
        }
    }

    #[test]
    fn stage_subset_accepts_full_and_selected_runnable_identity() -> Result<()> {
        validate(
            stage(),
            REVISION,
            &registry(),
            &[qualified("selected"), qualified("unselected")],
        )?;
        validate(stage(), REVISION, &registry(), &[qualified("selected")])?;
        // The selected target need not be the first original receipt row.
        validate(stage(), REVISION, &registry(), &[qualified("unselected")])?;
        Ok(())
    }

    #[test]
    fn stage_subset_rejects_missing_selected_target_and_changed_identity() {
        assert!(validate(stage(), REVISION, &registry(), &[qualified("missing")]).is_err());
        let mut changed = qualified("selected");
        changed.repository = "registry.example.invalid/other/selected".into();
        assert!(validate(stage(), REVISION, &registry(), &[changed]).is_err());
        let mut changed = qualified("selected");
        changed.digest = digest('e');
        assert!(validate(stage(), REVISION, &registry(), &[changed]).is_err());
    }

    #[test]
    fn stage_subset_rejects_duplicate_stage_targets_and_empty_selection() {
        for duplicate in ["selected", "unselected"] {
            let mut staged = stage();
            staged.images.push(row(duplicate));
            assert!(validate(staged, REVISION, &registry(), &[qualified("selected")]).is_err());
        }
        assert!(validate(stage(), REVISION, &registry(), &[]).is_err());
        let mut staged = stage();
        staged.images.clear();
        assert!(validate(staged, REVISION, &registry(), &[qualified("selected")]).is_err());
    }

    #[test]
    fn stage_subset_rejects_invalid_header_source_and_registry() {
        let changes: [fn(&mut ImageStageEvidenceInput); 6] = [
            |s| s.schema_version = "veoveo.ai/image-stage-evidence/v0".into(),
            |s| s.release_eligible = true,
            |s| s.source_revision = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
            |s| s.registry.push_address = "another.example.invalid".into(),
            |s| s.registry.pull_address = "another.example.invalid".into(),
            |s| s.registry.transport = RegistryTransport::InsecureHttp,
        ];
        for change in changes {
            let mut staged = stage();
            change(&mut staged);
            assert!(validate(staged, REVISION, &registry(), &[qualified("selected")]).is_err());
        }
    }

    #[test]
    fn stage_subset_rejects_invalid_selected_and_unselected_rows() {
        let changes: [fn(&mut StagedImageInput); 5] = [
            |r| r.platform = "linux/arm64".into(),
            |r| r.runtime_digest = "not-a-digest".into(),
            |r| r.runtime_digest = format!("sha256:{}", "B".repeat(64)),
            |r| r.staging_index_digest = "not-a-digest".into(),
            |r| r.staging_index_digest = "sha256:00".into(),
        ];
        for index in [0, 1] {
            for change in changes {
                let mut staged = stage();
                change(&mut staged.images[index]);
                assert!(validate(staged, REVISION, &registry(), &[qualified("selected")]).is_err());
            }
        }
    }
}
