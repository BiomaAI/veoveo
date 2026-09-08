//! Image-only lock publication preserves every other immutable input and owner.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use veoveo_deploy_contract::{DeploymentLock, LoadedProfile, LockedImage, components::*};

use crate::{
    compile::compile_image_update,
    images::validate_locked_images,
    sources::{resolve_component_sources, validate_locked_profile},
};

/// Composes qualified image updates into exactly the requested components.
///
/// The caller verifies the supplied OCI qualification evidence. This function
/// validates its artifact bindings, clones only requested chart sources, renders
/// their complete releases, and returns a complete lock. Dependencies and other
/// components are retained verbatim. It performs no build or Kubernetes operation.
/// The installation checkout must match the base lock's profile revision.
pub fn update_component_images(
    profile: &LoadedProfile,
    base: &DeploymentLock,
    requested: &BTreeSet<ComponentId>,
    images: &BTreeMap<String, Vec<LockedImage>>,
) -> Result<DeploymentLock> {
    base.validate()?;
    validate_locked_profile(profile, base)?;
    select_components(&base.components, requested)?;
    let (mut updated, replacements) = merge_images(base, requested, images)?;
    validate_locked_images(profile, &updated)?;
    let snapshots = resolve_component_sources(profile, base, requested)?;
    let roots = snapshots
        .iter()
        .map(|(identity, snapshot)| (identity.clone(), snapshot.repository.clone()))
        .collect();
    for compiled in compile_image_update(profile, &updated, &roots, requested, &replacements)? {
        let previous = updated
            .components
            .iter_mut()
            .find(|component| component.declaration.id == compiled.locked.declaration.id)
            .context("rendered component is outside the base lock")?;
        *previous = compiled.locked;
    }
    updated.validate()?;
    validate_profile_component_bindings(&profile.definition, &updated)?;
    Ok(updated)
}

type ImageReplacements = BTreeMap<(String, String), LockedImage>;

fn merge_images(
    base: &DeploymentLock,
    requested: &BTreeSet<ComponentId>,
    images: &BTreeMap<String, Vec<LockedImage>>,
) -> Result<(DeploymentLock, ImageReplacements)> {
    let consumers = base
        .components
        .iter()
        .filter(|component| requested.contains(&component.declaration.id))
        .flat_map(|component| &component.units)
        .flat_map(|unit| &unit.inputs)
        .filter_map(|input| match input {
            ComponentInput::Image { source, target, .. } => {
                Some((source.name.clone(), target.clone()))
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut updated = base.clone();
    let mut replacements = BTreeMap::new();
    for (name, images) in images {
        ensure!(
            !images.is_empty(),
            "image update source {name} contains no images"
        );
        let source = updated
            .sources
            .iter_mut()
            .find(|source| &source.name == name)
            .context("image update source is outside the base lock")?;
        for image in images {
            let key = (name.clone(), image.name.clone());
            ensure!(
                consumers.contains(&key),
                "image {name}/{} is not consumed by a requested component",
                image.name
            );
            ensure!(
                replacements.insert(key, image.clone()).is_none(),
                "duplicate image update for {name}/{}",
                image.name
            );
            let previous = source
                .images
                .iter()
                .find(|previous| previous.name == image.name)
                .context("image update target is outside the base lock")?;
            ensure!(
                previous.repository == image.repository,
                "image update changes target repository ownership"
            );
            if let Some(existing) = source.images.iter().find(|previous| {
                previous.name == image.name && previous.source_revision == image.source_revision
            }) {
                ensure!(
                    existing == image,
                    "image update conflicts with an already qualified build revision"
                );
            } else {
                source.images.push(image.clone());
            }
        }
    }
    ensure!(
        !replacements.is_empty(),
        "component publication requires qualified image updates"
    );
    // Validate the merged artifact catalog before opening any source checkout.
    updated.validate()?;
    Ok((updated, replacements))
}
