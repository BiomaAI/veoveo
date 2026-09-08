//! Exact component publication preserves every unrequested input and owner.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use veoveo_deploy_contract::{DeploymentLock, LoadedProfile, LockedImage, components::*};
use veoveo_extension_contract::SourceRevision;

use crate::{
    compile::{compile_component_update, configuration},
    images::validate_locked_images,
    sources::{resolve_component_sources, resolve_revision, validate_locked_profile},
};

/// Inputs that may change in the exact requested components. Empty maps retain
/// their existing artifact identities; configuration refresh uses the current
/// immutable installation checkout.
#[derive(Debug, Default)]
pub struct ComponentUpdates {
    pub images: BTreeMap<String, Vec<LockedImage>>,
    pub source_revisions: BTreeMap<String, SourceRevision>,
    pub refresh_configuration: bool,
}

/// Composes chart, configuration, and qualified image updates into requested components.
///
/// The caller verifies the supplied OCI qualification evidence. This function
/// validates its artifact bindings, clones only requested chart sources, renders
/// their complete releases, and returns a complete lock. Dependencies and other
/// components are retained verbatim. It performs no build or Kubernetes operation.
pub fn update_components(
    profile: &LoadedProfile,
    base: &DeploymentLock,
    requested: &BTreeSet<ComponentId>,
    updates: &ComponentUpdates,
) -> Result<DeploymentLock> {
    base.validate()?;
    ensure!(
        updates.refresh_configuration
            || !updates.source_revisions.is_empty()
            || !updates.images.is_empty(),
        "component publication requires a configuration, chart source, or image update"
    );
    let original = configuration::identity(profile, &base.profile_revision)?;
    ensure!(
        base.components.iter().all(|component| component
            .declaration
            .configuration
            .source
            .repository
            == original.source.repository),
        "component publication changes the installation repository owner"
    );
    let original_profiles =
        configuration::ConfigurationSnapshots::prepare(profile, std::iter::once(&original))?;
    let original_profile = original_profiles.get(&original)?;
    validate_locked_profile(original_profile, base)?;
    ensure!(
        profile.definition.components == original_profile.definition.components,
        "component publication cannot change ownership topology"
    );
    ensure!(
        profile.resolved_platform()? == base.platform,
        "component publication cannot change shared platform selection"
    );
    ensure!(
        profile.definition.sources.len() == original_profile.definition.sources.len()
            && profile
                .definition
                .sources
                .iter()
                .all(|source| original_profile
                    .definition
                    .sources
                    .iter()
                    .any(|previous| previous.name == source.name
                        && previous.role == source.role
                        && previous.repository == source.repository)),
        "component publication changes source repository ownership"
    );
    select_components(&base.components, requested)?;
    for name in updates.source_revisions.keys() {
        ensure!(
            base.components
                .iter()
                .any(|component| requested.contains(&component.declaration.id)
                    && component.declaration.source.name == *name
                    && component.declaration.role != ComponentRole::Installation),
            "chart source {name} is not owned by a requested component"
        );
    }
    let (mut updated, replacements) = merge_images(base, requested, &updates.images)?;
    updated.profile_revision = resolve_revision(&profile.repository, "HEAD")?;
    let snapshots = resolve_component_sources(profile, base, requested, &updates.source_revisions)?;
    let roots = snapshots
        .iter()
        .map(|(identity, snapshot)| (identity.clone(), snapshot.repository.clone()))
        .collect();
    for compiled in compile_component_update(
        profile,
        &mut updated,
        &roots,
        requested,
        &replacements,
        updates,
    )? {
        let previous = updated
            .components
            .iter_mut()
            .find(|component| component.declaration.id == compiled.locked.declaration.id)
            .context("rendered component is outside the base lock")?;
        *previous = compiled.locked;
    }
    updated.validate()?;
    validate_locked_images(profile, &updated)?;
    validate_locked_profile(profile, &updated)?;
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
    // Validate the merged artifact catalog before opening any source checkout.
    updated.validate()?;
    Ok((updated, replacements))
}
