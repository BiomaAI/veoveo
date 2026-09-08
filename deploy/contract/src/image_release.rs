use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_extension_contract::SourceRevision;

use crate::{LockedImage, LockedRegistry, ensure_unique, validate_digest, validate_name};

pub const IMAGE_RELEASE_EVIDENCE_SCHEMA: &str = "veoveo.io/image-release-evidence/v3";

/// Qualified images produced from one immutable source snapshot. Mixed-revision
/// installation closures use DeploymentLock instead of relabeling image provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageReleaseEvidence {
    pub schema_version: String,
    pub source_revision: SourceRevision,
    pub registry: LockedRegistry,
    pub images: Vec<LockedImage>,
}

impl ImageReleaseEvidence {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == IMAGE_RELEASE_EVIDENCE_SCHEMA,
            "image evidence schemaVersion must be {IMAGE_RELEASE_EVIDENCE_SCHEMA}"
        );
        self.registry.validate()?;
        ensure!(!self.images.is_empty(), "image release contains no images");
        ensure_unique(
            "published image target",
            self.images.iter().map(|image| &image.name),
        )?;
        ensure_unique(
            "published image repository",
            self.images.iter().map(|image| &image.repository),
        )?;
        for image in &self.images {
            validate_name("published image target", &image.name)?;
            ensure!(
                image.source_revision == self.source_revision,
                "published image build provenance differs from the release snapshot"
            );
            let prefix = format!("{}/", self.registry.pull_address);
            ensure!(
                image.repository.starts_with(&prefix)
                    && image.repository.len() > prefix.len()
                    && !image.repository.contains('@')
                    && !image.repository.chars().any(char::is_whitespace)
                    && !image
                        .repository
                        .rsplit('/')
                        .next()
                        .unwrap_or_default()
                        .contains(':'),
                "published image repository must be untagged and inside the declared pull registry"
            );
            validate_digest(&image.digest)?;
            validate_digest(&image.publication_digest)?;
            ensure!(
                image.digest != image.publication_digest,
                "published image must distinguish runnable and attested publication digests"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> ImageReleaseEvidence {
        let revision = SourceRevision::new("a".repeat(40)).unwrap();
        ImageReleaseEvidence {
            schema_version: IMAGE_RELEASE_EVIDENCE_SCHEMA.into(),
            source_revision: revision.clone(),
            registry: LockedRegistry {
                push_address: "registry.example.invalid".into(),
                pull_address: "registry.example.invalid".into(),
                transport: crate::RegistryTransport::Tls,
            },
            images: vec![LockedImage {
                name: "fixture".into(),
                repository: "registry.example.invalid/fixture".into(),
                source_revision: revision,
                digest: format!("sha256:{}", "b".repeat(64)),
                publication_digest: format!("sha256:{}", "c".repeat(64)),
            }],
        }
    }

    #[test]
    fn publication_roundtrip_preserves_both_artifact_identities_and_build_provenance() {
        let original = fixture();
        original.validate().unwrap();
        let bytes = serde_json::to_vec(&original).unwrap();
        let parsed: ImageReleaseEvidence = serde_json::from_slice(&bytes).unwrap();
        parsed.validate().unwrap();
        assert_eq!(parsed, original);
        let mut relabeled = parsed;
        relabeled.images[0].source_revision = SourceRevision::new("d".repeat(40)).unwrap();
        assert!(
            relabeled
                .validate()
                .unwrap_err()
                .to_string()
                .contains("build provenance")
        );
    }

    #[test]
    fn publication_rejects_ambiguous_targets_unqualified_digests_and_foreign_registries() {
        let original = fixture();
        let mut duplicate = original.clone();
        duplicate.images.push(duplicate.images[0].clone());
        assert!(duplicate.validate().is_err());
        let mut runtime_only = original.clone();
        runtime_only.images[0].publication_digest = runtime_only.images[0].digest.clone();
        assert!(runtime_only.validate().is_err());
        let mut foreign = original.clone();
        foreign.images[0].repository = "registry.example.invalid.attacker.invalid/fixture".into();
        assert!(foreign.validate().is_err());
        let mut tagged = original;
        tagged.images[0].repository.push_str(":mutable");
        assert!(tagged.validate().is_err());
    }
}
