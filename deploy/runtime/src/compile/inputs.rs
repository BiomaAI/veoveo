//! Chart snapshots are keyed by the component's complete immutable source identity.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use anyhow::{Context, Result, ensure};
use veoveo_deploy_contract::{DeploymentLock, LoadedProfile, LockedSource, components::*};

use crate::{
    charts::validate_locked_charts,
    images::locked_image_digests,
    sources::{ResolvedSource, SourceCheckout, resolve_revision},
};

pub(super) struct CompilationInputs {
    pub snapshots: BTreeMap<ComponentId, ResolvedSource>,
    pub owners: BTreeMap<ComponentId, ComponentSource>,
}

pub(super) fn publication(
    profile: &LoadedProfile,
    profile_revision: &str,
    sources: &[LockedSource],
    roots: &BTreeMap<String, PathBuf>,
    selected: &BTreeSet<ComponentId>,
) -> Result<CompilationInputs> {
    let mut owners = BTreeMap::new();
    let mut snapshots = BTreeMap::new();
    for spec in profile
        .definition
        .components
        .iter()
        .filter(|spec| selected.contains(&spec.id))
    {
        let owner = match &spec.owner {
            ComponentOwner::Source { name } => {
                let source = sources
                    .iter()
                    .find(|source| &source.name == name)
                    .context("component source has no qualified artifacts")?;
                let owner = super::component_source(source)?;
                snapshots.insert(
                    owner.clone(),
                    roots
                        .get(name)
                        .context("publication omits a source snapshot")?
                        .clone(),
                );
                owner
            }
            ComponentOwner::Installation => super::installation_source(profile, profile_revision)?,
        };
        owners.insert(spec.id.clone(), owner);
    }
    prepare(profile, sources, &snapshots, selected, owners)
}

pub(super) fn locked(
    profile: &LoadedProfile,
    lock: &DeploymentLock,
    roots: &BTreeMap<ComponentSource, PathBuf>,
    selected: &BTreeSet<ComponentId>,
) -> Result<CompilationInputs> {
    let owners = lock
        .components
        .iter()
        .filter(|component| selected.contains(&component.declaration.id))
        .map(|component| {
            (
                component.declaration.id.clone(),
                component.declaration.source.clone(),
            )
        })
        .collect();
    prepare(profile, &lock.sources, roots, selected, owners)
}

fn prepare(
    profile: &LoadedProfile,
    sources: &[LockedSource],
    roots: &BTreeMap<ComponentSource, PathBuf>,
    selected: &BTreeSet<ComponentId>,
    owners: BTreeMap<ComponentId, ComponentSource>,
) -> Result<CompilationInputs> {
    selected_source_releases(&profile.definition, selected)?;
    ensure!(
        owners.keys().cloned().collect::<BTreeSet<_>>() == *selected,
        "component source identities do not cover the exact selection"
    );
    let deployment_images = locked_image_digests(profile, sources)?;
    let mut snapshots = BTreeMap::new();
    for spec in profile
        .definition
        .components
        .iter()
        .filter(|spec| selected.contains(&spec.id))
    {
        let ComponentOwner::Source { name } = &spec.owner else {
            continue;
        };
        let owner = &owners[&spec.id];
        let source = sources
            .iter()
            .find(|source| &source.name == name)
            .context("component source is outside the artifact catalog")?;
        ensure!(
            owner.name == *name && owner.repository == source.repository,
            "component source identity differs from the artifact catalog"
        );
        let root = roots
            .get(owner)
            .context("component has no exact source snapshot")?;
        ensure!(
            resolve_revision(root, "HEAD")? == owner.revision.as_str(),
            "component source snapshot differs from its locked revision"
        );
        let mut definition = profile
            .definition
            .sources
            .iter()
            .find(|source| &source.name == name)
            .context("component source is outside the profile")?
            .clone();
        definition
            .releases
            .retain(|release| spec.releases.contains(&release.name));
        validate_locked_charts(&definition, source, root)?;
        snapshots.insert(
            spec.id.clone(),
            ResolvedSource {
                definition,
                repository: root.clone(),
                revision: owner.revision.as_str().to_owned(),
                image_digests: locked_image_digests(profile, std::slice::from_ref(source))?,
                deployment_image_digests: deployment_images.clone(),
                _checkout: SourceCheckout::Publication,
            },
        );
    }
    Ok(CompilationInputs { snapshots, owners })
}
