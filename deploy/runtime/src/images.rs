use crate::sources::ResolvedSource;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{collections::BTreeMap, process::Command};
use veoveo_deploy_contract::{
    DeploymentLock, DeploymentSourceRole, LoadedProfile, LockedSource, PlannedImage,
};

#[derive(Debug, Deserialize)]
struct BakePrint {
    group: BTreeMap<String, BakeGroup>,
    target: BTreeMap<String, BakeImageTarget>,
}

#[derive(Debug, Deserialize)]
struct BakeGroup {
    targets: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BakeImageTarget {
    #[serde(default)]
    tags: Vec<String>,
}

pub(crate) fn locked_image_digests(
    profile: &LoadedProfile,
    sources: &[LockedSource],
) -> Result<BTreeMap<String, String>> {
    locked_image_digests_for_registry(&profile.definition.registry.pull_address, sources)
}

pub(crate) fn locked_image_digests_for_registry(
    registry: &str,
    sources: &[LockedSource],
) -> Result<BTreeMap<String, String>> {
    let prefix = format!("{registry}/");
    let mut image_digests = BTreeMap::new();
    for source in sources {
        for image in &source.images {
            let repository = image.repository.strip_prefix(&prefix).with_context(|| {
                format!(
                    "locked image {} repository {} is outside profile registry {}",
                    image.name, image.repository, registry
                )
            })?;
            ensure!(
                image_digests
                    .insert(repository.to_owned(), image.digest.clone())
                    .is_none(),
                "locked image repository {} is owned by more than one deployment source",
                image.repository
            );
        }
    }
    Ok(image_digests)
}

pub(crate) fn validate_locked_images(
    profile: &LoadedProfile,
    lock: &DeploymentLock,
    sources: &[ResolvedSource],
    planned: &[PlannedImage],
) -> Result<()> {
    let locked_count = lock
        .sources
        .iter()
        .map(|source| source.images.len())
        .sum::<usize>();
    ensure!(
        locked_count == planned.len(),
        "deployment lock contains {locked_count} images, selected Bake targets resolve {}",
        planned.len()
    );
    let mut locked_images = BTreeMap::new();
    for source in &lock.sources {
        for image in &source.images {
            locked_images.insert(
                (source.name.as_str(), image.name.as_str()),
                image.repository.as_str(),
            );
        }
    }
    for image in planned {
        let source = sources
            .iter()
            .find(|candidate| candidate.definition.name == image.source)
            .with_context(|| format!("planned image references unknown source {}", image.source))?;
        let repository = image
            .reference
            .strip_suffix(&format!(":{}", source.revision))
            .with_context(|| {
                format!(
                    "planned image {} does not use locked source revision {}",
                    image.reference, source.revision
                )
            })?;
        let locked_repository = locked_images
            .get(&(image.source.as_str(), image.target.as_str()))
            .with_context(|| {
                format!(
                    "deployment lock omits selected image {}:{}",
                    image.source, image.target
                )
            })?;
        ensure!(
            repository == *locked_repository,
            "deployment lock repository for {}:{} is {}, Bake resolves {}",
            image.source,
            image.target,
            locked_repository,
            repository
        );
        ensure!(
            repository.starts_with(&format!("{}/", profile.definition.registry.pull_address)),
            "locked image repository {repository} is outside profile registry {}",
            profile.definition.registry.pull_address
        );
    }
    Ok(())
}

pub(crate) fn validate_bake_selections(
    profile: &LoadedProfile,
    sources: &[ResolvedSource],
) -> Result<Vec<PlannedImage>> {
    let mut selected_images = Vec::new();
    let platform_targets = profile.required_platform_images()?;
    for source in sources {
        let selections = match source.definition.role {
            DeploymentSourceRole::Platform => {
                vec![(
                    "exact platform selection".to_owned(),
                    platform_targets.iter().cloned().collect::<Vec<_>>(),
                )]
            }
            DeploymentSourceRole::Extension | DeploymentSourceRole::Workload => source
                .definition
                .image_groups
                .iter()
                .map(|group| (format!("group {group}"), vec![group.clone()]))
                .collect(),
        };
        for (selection_name, bake_patterns) in selections {
            let mut command = Command::new("docker");
            command.args(["buildx", "bake"]);
            command.args(&bake_patterns);
            let output = command
                .arg("--print")
                .current_dir(&source.repository)
                .env("VEOVEO_REGISTRY", &profile.definition.registry.pull_address)
                .env("VEOVEO_IMAGE_TAG", &source.revision)
                .output()
                .with_context(|| {
                    format!(
                        "running Docker Bake {selection_name} from source {} in profile {}",
                        source.definition.name, profile.definition.name
                    )
                })?;
            ensure!(
                output.status.success(),
                "validating Docker Bake {selection_name} from source {} in profile {} failed:\n{}",
                source.definition.name,
                profile.definition.name,
                String::from_utf8_lossy(&output.stderr)
            );
            let definition =
                serde_json::from_slice::<BakePrint>(&output.stdout).with_context(|| {
                    format!(
                        "decoding Docker Bake {selection_name} from source {}",
                        source.definition.name
                    )
                })?;
            let selected_targets = if source.definition.role == DeploymentSourceRole::Platform {
                platform_targets.iter().cloned().collect::<Vec<_>>()
            } else {
                let group = bake_patterns
                    .first()
                    .expect("extension and workload selections contain one group");
                definition
                    .group
                    .get(group)
                    .with_context(|| format!("Docker Bake output omitted selected group {group}"))?
                    .targets
                    .clone()
            };
            for target in selected_targets {
                let image = definition.target.get(&target).with_context(|| {
                    format!("Docker Bake {selection_name} references missing target {target}")
                })?;
                ensure!(
                    image.tags.len() == 1,
                    "image target {target} from source {} must resolve exactly one OCI reference",
                    source.definition.name
                );
                selected_images.push(PlannedImage {
                    source: source.definition.name.clone(),
                    target,
                    reference: image
                        .tags
                        .first()
                        .expect("one image tag was required")
                        .clone(),
                });
            }
        }
    }
    Ok(selected_images)
}
