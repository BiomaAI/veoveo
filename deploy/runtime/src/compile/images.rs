//! Image selection belongs to an atomic release, independent of chart snapshots.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use veoveo_deploy_contract::{LockedImage, LockedSource, ReleaseValuesContract, components::*};
use veoveo_extension_contract::ArtifactDigest;

use super::objects::container_images;

pub(super) struct ImageInputs {
    pub digests: BTreeMap<String, String>,
    by_repository: BTreeMap<String, ComponentInput>,
    exact: bool,
}

impl ImageInputs {
    pub fn publication(
        registry: &str,
        owner: &str,
        contract: ReleaseValuesContract,
        sources: &[LockedSource],
    ) -> Result<Self> {
        let inputs = sources
            .iter()
            .filter(|source| contract == ReleaseValuesContract::Extension || source.name == owner)
            .flat_map(|source| source.images.iter().map(move |image| (source, image)))
            .map(|(source, image)| {
                Ok(ComponentInput::Image {
                    source: ComponentSource {
                        name: source.name.clone(),
                        repository: source.repository.clone(),
                        revision: image.source_revision.clone(),
                    },
                    target: image.name.clone(),
                    repository: image.repository.clone(),
                    digest: ArtifactDigest::new(&image.digest)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Self::new(registry, inputs, false)
    }

    pub fn locked(registry: &str, unit: &LockedAtomicUnit) -> Result<Self> {
        Self::new(
            registry,
            unit.inputs
                .iter()
                .filter(|input| matches!(input, ComponentInput::Image { .. }))
                .cloned(),
            true,
        )
    }

    pub fn updated(
        registry: &str,
        unit: &LockedAtomicUnit,
        updates: &BTreeMap<(String, String), LockedImage>,
    ) -> Result<Self> {
        let mut inputs = unit.inputs.clone();
        inputs = inputs
            .into_iter()
            .map(|mut input| {
                if let ComponentInput::Image {
                    source,
                    target,
                    digest,
                    ..
                } = &mut input
                    && let Some(image) = updates.get(&(source.name.clone(), target.clone()))
                {
                    source.revision = image.source_revision.clone();
                    *digest = ArtifactDigest::new(&image.digest)?;
                }
                Ok(input)
            })
            .collect::<Result<_>>()?;
        Self::new(
            registry,
            inputs
                .into_iter()
                .filter(|input| matches!(input, ComponentInput::Image { .. })),
            true,
        )
    }

    fn new(
        registry: &str,
        inputs: impl IntoIterator<Item = ComponentInput>,
        exact: bool,
    ) -> Result<Self> {
        let mut digests = BTreeMap::new();
        let mut by_repository = BTreeMap::new();
        let prefix = format!("{registry}/");
        for input in inputs {
            let ComponentInput::Image {
                repository, digest, ..
            } = &input
            else {
                anyhow::bail!("image selection contains a non-image input");
            };
            let path = repository
                .strip_prefix(&prefix)
                .context("release image is outside the installation registry")?;
            ensure!(
                digests
                    .insert(path.to_owned(), digest.as_str().to_owned())
                    .is_none(),
                "release image selection contains multiple versions or build provenances for {repository}"
            );
            by_repository.insert(repository.clone(), input);
        }
        Ok(Self {
            digests,
            by_repository,
            exact,
        })
    }

    pub fn rendered(&self, registry: &str, objects: &[Value]) -> Result<BTreeSet<ComponentInput>> {
        let mut inputs = BTreeSet::new();
        for reference in container_images(objects)? {
            if !reference.starts_with(&format!("{registry}/")) {
                continue;
            }
            let (repository, digest) = reference
                .split_once('@')
                .context("component uses a mutable source image")?;
            let input = self
                .by_repository
                .get(repository)
                .context("component image is outside its release input closure")?;
            let ComponentInput::Image {
                digest: expected, ..
            } = input
            else {
                unreachable!("only image inputs enter this map");
            };
            ensure!(
                expected.as_str() == digest,
                "rendered component image differs from its selected artifact"
            );
            inputs.insert(input.clone());
        }
        if self.exact {
            ensure!(
                inputs.len() == self.by_repository.len(),
                "rendered release omits a locked image input"
            );
        }
        Ok(inputs)
    }

    /// Publication discovers consumers with its candidate inventory, then renders
    /// the exact values installation will use. That second render must consume
    /// precisely the discovered images; template-dependent image expansion fails.
    pub fn narrow(
        &self,
        registry: &str,
        consumed: &BTreeSet<ComponentInput>,
    ) -> Result<Option<Self>> {
        if self.exact || consumed.len() == self.by_repository.len() {
            return Ok(None);
        }
        Self::new(registry, consumed.iter().cloned(), true).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_deploy_contract::{DeploymentSourceRole, LockedImage};
    use veoveo_extension_contract::SourceRevision;

    fn source(name: &str, revision: char) -> LockedSource {
        LockedSource {
            name: name.into(),
            role: DeploymentSourceRole::Workload,
            repository: format!("https://example.invalid/{name}"),
            revision: revision.to_string().repeat(40),
            images: vec![LockedImage {
                name: name.into(),
                repository: format!("registry.example/{name}"),
                source_revision: SourceRevision::new(revision.to_string().repeat(40)).unwrap(),
                digest: format!("sha256:{}", "a".repeat(64)),
                publication_digest: format!("sha256:{}", "b".repeat(64)),
            }],
            charts: vec![],
        }
    }

    fn workload(image: &str) -> Vec<Value> {
        vec![
            serde_json::json!({"apiVersion":"apps/v1", "kind":"Deployment", "spec":{"template":{"spec":{"containers":[{"name":"app", "image":image}]}}}}),
        ]
    }

    #[test]
    fn publication_candidates_follow_the_release_values_contract() {
        let sources = [source("platform", 'a'), source("extension", 'b')];
        for contract in [
            ReleaseValuesContract::Platform,
            ReleaseValuesContract::VeoveoSource,
        ] {
            let selected =
                ImageInputs::publication("registry.example", "platform", contract, &sources)
                    .unwrap();
            assert_eq!(
                selected
                    .digests
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["platform"]
            );
        }
        let selected = ImageInputs::publication(
            "registry.example",
            "extension",
            ReleaseValuesContract::Extension,
            &sources,
        )
        .unwrap();
        assert_eq!(
            selected
                .digests
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["extension", "platform"]
        );
        let used = selected
            .rendered(
                "registry.example",
                &workload(&format!(
                    "registry.example/platform@{}",
                    sources[0].images[0].digest
                )),
            )
            .unwrap();
        assert_eq!(used.len(), 1);
    }

    #[test]
    fn locked_selection_rejects_unused_foreign_changed_and_ambiguous_inputs() {
        let sources = [source("platform", 'a')];
        let publication = ImageInputs::publication(
            "registry.example",
            "platform",
            ReleaseValuesContract::Platform,
            &sources,
        )
        .unwrap();
        let input = publication.by_repository.values().next().unwrap().clone();
        let locked = ImageInputs::new("registry.example", [input.clone()], true).unwrap();
        let exact = format!("registry.example/platform@{}", sources[0].images[0].digest);
        assert_eq!(
            locked
                .rendered("registry.example", &workload(&exact))
                .unwrap(),
            BTreeSet::from([input.clone()])
        );
        assert!(locked.rendered("registry.example", &[]).is_err());
        assert!(
            locked
                .rendered(
                    "registry.example",
                    &workload("registry.example/unselected@sha256:bad")
                )
                .is_err()
        );
        assert!(
            locked
                .rendered(
                    "registry.example",
                    &workload("registry.example/platform:mutable")
                )
                .is_err()
        );
        assert!(
            locked
                .rendered(
                    "registry.example",
                    &workload("registry.example/platform@sha256:changed")
                )
                .is_err()
        );
        let mut newer = input.clone();
        if let ComponentInput::Image { source, .. } = &mut newer {
            source.revision = SourceRevision::new("c".repeat(40)).unwrap();
        }
        assert!(ImageInputs::new("registry.example", [input.clone(), newer], true).is_err());
        assert!(ImageInputs::new("different.example", [input], true).is_err());
    }
}
