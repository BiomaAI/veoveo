use crate::sources::ResolvedSource;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{collections::BTreeMap, process::Command};
use veoveo_deploy_contract::{DeploymentLock, DeploymentSourceRole, LoadedProfile, PlannedImage};

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

pub(crate) fn validate_locked_images(profile: &LoadedProfile, lock: &DeploymentLock) -> Result<()> {
    lock.validate()?;
    // Validate target ownership and platform completeness once per target. Actual
    // artifact selection comes from each atomic unit, including retained versions.
    let mut targets = BTreeMap::new();
    for source in &lock.sources {
        for image in &source.images {
            targets
                .entry((&source.name, &image.name))
                .or_insert_with(|| PlannedImage {
                    source: source.name.clone(),
                    target: image.name.clone(),
                    reference: format!("{}@{}", image.repository, image.digest),
                });
        }
    }
    profile.validate_image_plan(&targets.into_values().collect::<Vec<_>>())
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
