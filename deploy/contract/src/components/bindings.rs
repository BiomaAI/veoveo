//! Cross-check the retained catalog against installation permissions and artifacts.
//! These checks use the complete lock without rendering unselected components.

use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};

use super::*;
use crate::{DeploymentLock, DeploymentProfile, DeploymentSourceRole};

pub fn validate_profile_component_bindings(
    profile: &DeploymentProfile,
    lock: &DeploymentLock,
) -> Result<()> {
    validate_profile_components(profile, &profile.components)?;
    ensure!(
        profile
            .components
            .iter()
            .map(|component| &component.id)
            .collect::<BTreeSet<_>>()
            == lock
                .components
                .iter()
                .map(|component| &component.declaration.id)
                .collect(),
        "deployment lock component owners differ from the complete profile catalog"
    );
    for spec in &profile.components {
        let component = lock
            .components
            .iter()
            .find(|component| component.declaration.id == spec.id)
            .context("profile component has no locked inventory")?;
        let declaration = &component.declaration;
        ensure!(
            declaration.role == spec.role
                && declaration.namespaces == spec.namespaces
                && declaration.dependencies == spec.dependencies
                && declaration.extension_release == spec.extension_release,
            "component {} metadata differs from its profile declaration",
            spec.id
        );
        let owner = match &spec.owner {
            ComponentOwner::Source { name } => name.as_str(),
            ComponentOwner::Installation => INSTALLATION_SOURCE_NAME,
        };
        ensure!(
            declaration.source.name == owner,
            "component {} source differs from its profile owner",
            spec.id
        );
        let expected_targets = match &spec.owner {
            ComponentOwner::Source { .. } => spec
                .releases
                .iter()
                .map(|name| AtomicTarget::HelmRelease {
                    namespace: profile.namespace.clone(),
                    name: name.clone(),
                })
                .collect(),
            ComponentOwner::Installation => spec
                .installation_inputs
                .iter()
                .map(|input| input.target(profile))
                .collect::<Result<BTreeSet<_>>>()?,
        };
        ensure!(
            declaration.targets == expected_targets,
            "component {} atomic targets differ from its profile operations",
            spec.id
        );
        let mut expected_permissions = spec.cluster_objects.clone();
        for object in component.units.iter().flat_map(|unit| &unit.objects) {
            match &object.identity.namespace {
                Some(namespace) => ensure!(
                    spec.namespaces.contains(namespace),
                    "component {} renders an undeclared namespace",
                    spec.id
                ),
                None => ensure!(
                    spec.cluster_objects.contains(&object.identity),
                    "component {} renders an undeclared cluster object",
                    spec.id
                ),
            }
            expected_permissions.insert(object.identity.clone());
        }
        ensure!(
            declaration.permitted_objects == expected_permissions,
            "component {} permissions differ from its profile and complete inventory",
            spec.id
        );
    }
    Ok(())
}

pub(crate) fn validate_artifact_bindings(lock: &DeploymentLock) -> Result<()> {
    let installation = lock
        .components
        .iter()
        .find(|component| component.declaration.role == ComponentRole::Installation)
        .map(|component| &component.declaration.source)
        .context("deployment lock has no installation owner")?;
    let mut charts_used = BTreeSet::new();
    for component in &lock.components {
        let declaration = &component.declaration;
        let owner = &declaration.source;
        if owner.name == INSTALLATION_SOURCE_NAME {
            ensure!(
                declaration.role == ComponentRole::Installation
                    && owner.repository == installation.repository,
                "installation component source identity is inconsistent"
            );
        } else {
            let source = lock
                .sources
                .iter()
                .find(|source| source.name == owner.name)
                .context("component owner is outside the locked sources")?;
            let role = match source.role {
                DeploymentSourceRole::Platform => ComponentRole::Platform,
                DeploymentSourceRole::Extension => ComponentRole::Extension,
                DeploymentSourceRole::Workload => ComponentRole::Workload,
            };
            ensure!(
                declaration.role == role && owner.repository == source.repository,
                "component owner differs from its locked source identity"
            );
        }
        for unit in &component.units {
            for input in &unit.inputs {
                match input {
                    ComponentInput::Image {
                        source,
                        target,
                        repository,
                        digest,
                    } => {
                        if source.name == INSTALLATION_SOURCE_NAME {
                            let allocator = &lock
                                .platform
                                .gpu_scheduling
                                .as_ref()
                                .context("installation image has no managed allocator")?
                                .allocator
                                .installation;
                            ensure!(
                                owner == source
                                    && repository == &allocator.image.repository
                                    && digest.as_str() == allocator.image.digest
                                    && target == "nvidia-dra-driver-gpu",
                                "installation image differs from the managed allocator artifact"
                            );
                        } else {
                            let locked = lock
                                .sources
                                .iter()
                                .find(|locked| locked.name == source.name)
                                .context(
                                    "component image source is outside the artifact closure",
                                )?;
                            let image = locked
                                .images
                                .iter()
                                .find(|image| &image.name == target)
                                .context(
                                    "component image target is outside the artifact closure",
                                )?;
                            ensure!(
                                source.repository == locked.repository
                                    && source.revision == image.source_revision
                                    && repository == &image.repository
                                    && digest.as_str() == image.digest,
                                "component image differs from its locked artifact or build provenance"
                            );
                        }
                    }
                    ComponentInput::Chart {
                        source,
                        coordinate,
                        digest,
                    } => {
                        ensure!(
                            source == owner,
                            "component chart is outside its owning source snapshot"
                        );
                        let AtomicTarget::HelmRelease { namespace, name } = &unit.target else {
                            anyhow::bail!("chart input belongs to a non-Helm operation");
                        };
                        if source.name == INSTALLATION_SOURCE_NAME {
                            let allocator = &lock
                                .platform
                                .gpu_scheduling
                                .as_ref()
                                .context("installation chart has no managed allocator")?
                                .allocator
                                .installation;
                            ensure!(
                                name == &allocator.release_name
                                    && namespace == &allocator.namespace
                                    && coordinate
                                        == &format!(
                                            "{}:{}@{}",
                                            allocator.chart.coordinate,
                                            allocator.chart.version,
                                            allocator.chart.digest
                                        )
                                    && digest.as_str() == allocator.chart.content_digest,
                                "installation chart differs from the managed allocator artifact"
                            );
                        } else {
                            let locked = lock
                                .sources
                                .iter()
                                .find(|locked| locked.name == source.name)
                                .context(
                                    "component chart source is outside the artifact closure",
                                )?;
                            let chart = locked
                                .charts
                                .iter()
                                .find(|chart| &chart.release == name)
                                .context(
                                "component release is outside its source artifact closure",
                            )?;
                            ensure!(
                                coordinate == &chart.coordinate && digest.as_str() == chart.digest,
                                "component chart differs from its locked artifact"
                            );
                            charts_used.insert((&locked.name, &chart.release));
                        }
                    }
                    ComponentInput::File { source, .. } => {
                        ensure!(
                            source == owner
                                || (source.name == INSTALLATION_SOURCE_NAME
                                    && source.repository == installation.repository),
                            "component values input is outside its source or installation owner"
                        );
                    }
                }
            }
            if matches!(unit.target, AtomicTarget::HelmRelease { .. }) {
                ensure!(
                    unit.inputs
                        .iter()
                        .filter(|input| matches!(input, ComponentInput::Chart { .. }))
                        .count()
                        == 1,
                    "Helm operation must consume exactly one locked chart"
                );
            }
        }
    }
    ensure!(
        charts_used
            == lock
                .sources
                .iter()
                .flat_map(|source| source
                    .charts
                    .iter()
                    .map(|chart| (&source.name, &chart.release)))
                .collect(),
        "locked charts differ from the complete component release inventory"
    );
    Ok(())
}
