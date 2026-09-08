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

use super::configuration::{self, ConfigurationSnapshots};
use super::images::ImageInputs;

pub(super) struct CompilationInputs {
    pub snapshots: BTreeMap<ComponentId, ResolvedSource>,
    pub owners: BTreeMap<ComponentId, ComponentSource>,
    pub images: BTreeMap<AtomicTarget, ImageInputs>,
    pub configurations: BTreeMap<ComponentId, InstallationSnapshot>,
    pub installation: ConfigurationSnapshots,
}

pub(super) fn publication(
    profile: &LoadedProfile,
    profile_revision: &str,
    sources: &[LockedSource],
    roots: &BTreeMap<String, PathBuf>,
    selected: &BTreeSet<ComponentId>,
) -> Result<CompilationInputs> {
    selected_source_releases(&profile.definition, selected)?;
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
    let configuration = configuration::identity(profile, profile_revision)?;
    let configurations: BTreeMap<_, _> = selected
        .iter()
        .map(|id| (id.clone(), configuration.clone()))
        .collect();
    let installation = ConfigurationSnapshots::prepare(profile, configurations.values())?;
    let mut prepared = prepare(
        profile,
        sources,
        &snapshots,
        selected,
        owners,
        configurations,
        installation,
    )?;
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
    selected_source_releases(&profile.definition, selected)?;
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
    let configurations: BTreeMap<_, _> = lock
        .components
        .iter()
        .filter(|component| selected.contains(&component.declaration.id))
        .map(|component| {
            (
                component.declaration.id.clone(),
                component.declaration.configuration.clone(),
            )
        })
        .collect();
    let installation = ConfigurationSnapshots::prepare(profile, configurations.values())?;
    let mut prepared = prepare(
        profile,
        &lock.sources,
        roots,
        selected,
        owners,
        configurations,
        installation,
    )?;
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

pub(super) fn component_update(
    profile: &LoadedProfile,
    lock: &mut DeploymentLock,
    roots: &BTreeMap<ComponentSource, PathBuf>,
    selected: &BTreeSet<ComponentId>,
    updates: &crate::publication::ComponentUpdates,
) -> Result<CompilationInputs> {
    let current = configuration::identity(profile, &lock.profile_revision)?;
    let mut owners = BTreeMap::new();
    let mut configurations = BTreeMap::new();
    for component in lock
        .components
        .iter()
        .filter(|component| selected.contains(&component.declaration.id))
    {
        let declaration = &component.declaration;
        let configuration = if updates.refresh_configuration {
            current.clone()
        } else {
            declaration.configuration.clone()
        };
        let mut owner = declaration.source.clone();
        if declaration.role == ComponentRole::Installation {
            owner = configuration.source.clone();
        } else if let Some(revision) = updates.source_revisions.get(&owner.name) {
            owner.revision = revision.clone();
        }
        owners.insert(declaration.id.clone(), owner);
        configurations.insert(declaration.id.clone(), configuration);
    }
    let installation = ConfigurationSnapshots::prepare(profile, configurations.values())?;
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
        let configuration = installation.get(&configurations[&spec.id])?;
        let mut definition = configuration
            .definition
            .sources
            .iter()
            .find(|source| source.name == *name)
            .context("updated component has no configured source")?
            .clone();
        definition
            .releases
            .retain(|release| spec.releases.contains(&release.name));
        let root = roots
            .get(owner)
            .context("updated chart source has no exact checkout")?;
        let charts = crate::charts::lock_source_charts(&definition, root)?;
        let source = lock
            .sources
            .iter_mut()
            .find(|source| source.name == *name)
            .context("updated chart source is outside the artifact catalog")?;
        for chart in charts {
            let previous = source
                .charts
                .iter_mut()
                .find(|previous| previous.release == chart.release)
                .context("updated chart release is outside the artifact catalog")?;
            if !updates.refresh_configuration && !updates.source_revisions.contains_key(name) {
                ensure!(
                    previous == &chart,
                    "retained chart inputs differ from their locked identity"
                );
            }
            *previous = chart;
        }
        if let Some(revision) = updates.source_revisions.get(name) {
            source.revision = revision.as_str().to_owned();
        }
    }
    // All other releases keep their chart locks, even when they share this Git source.
    prepare(
        profile,
        &lock.sources,
        roots,
        selected,
        owners,
        configurations,
        installation,
    )
}

fn prepare(
    profile: &LoadedProfile,
    sources: &[LockedSource],
    roots: &BTreeMap<ComponentSource, PathBuf>,
    selected: &BTreeSet<ComponentId>,
    owners: BTreeMap<ComponentId, ComponentSource>,
    configurations: BTreeMap<ComponentId, InstallationSnapshot>,
    installation: ConfigurationSnapshots,
) -> Result<CompilationInputs> {
    ensure!(
        owners.keys().cloned().collect::<BTreeSet<_>>() == *selected,
        "component source identities do not cover the exact selection"
    );
    ensure!(
        configurations.keys().cloned().collect::<BTreeSet<_>>() == *selected,
        "component configurations do not cover the exact selection"
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
        let mut definition = installation
            .get(&configurations[&spec.id])?
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
        configurations,
        installation,
    })
}
