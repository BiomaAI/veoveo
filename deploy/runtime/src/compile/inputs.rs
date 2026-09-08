//! Chart snapshots are keyed by the component's complete immutable source identity.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use anyhow::{Context, Result, ensure};
use veoveo_deploy_contract::{DeploymentLock, LoadedProfile, LockedSource, components::*};

use crate::{
    charts::validate_locked_charts,
    sources::{ResolvedSource, SourceCheckout, resolve_revision},
};

use super::images::ImageInputs;

pub(super) struct CompilationInputs {
    pub snapshots: BTreeMap<ComponentId, ResolvedSource>,
    pub owners: BTreeMap<ComponentId, ComponentSource>,
    pub images: BTreeMap<AtomicTarget, ImageInputs>,
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
    let mut prepared = prepare(profile, sources, &snapshots, selected, owners)?;
    for snapshot in prepared.snapshots.values() {
        for release in &snapshot.definition.releases {
            prepared.images.insert(
                AtomicTarget::HelmRelease {
                    namespace: profile.definition.namespace.clone(),
                    name: release.name.clone(),
                },
                ImageInputs::publication(
                    &profile.definition.registry.pull_address,
                    &snapshot.definition.name,
                    release.values_contract,
                    sources,
                )?,
            );
        }
    }
    Ok(prepared)
}

pub(super) fn locked(
    profile: &LoadedProfile,
    lock: &DeploymentLock,
    roots: &BTreeMap<ComponentSource, PathBuf>,
    selected: &BTreeSet<ComponentId>,
) -> Result<CompilationInputs> {
    lock.validate()?;
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
    let mut prepared = prepare(profile, &lock.sources, roots, selected, owners)?;
    for component in lock
        .components
        .iter()
        .filter(|component| prepared.snapshots.contains_key(&component.declaration.id))
    {
        for unit in &component.units {
            prepared.images.insert(
                unit.target.clone(),
                ImageInputs::locked(&profile.definition.registry.pull_address, unit)?,
            );
        }
    }
    Ok(prepared)
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
                _checkout: SourceCheckout::Publication,
            },
        );
    }
    Ok(CompilationInputs {
        snapshots,
        owners,
        images: BTreeMap::new(),
    })
}
