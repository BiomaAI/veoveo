use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ComponentExtensionRelease, ComponentId, ComponentRole, ObjectIdentity};
use crate::{DeploymentProfile, DeploymentSourceRole};

/// Repository whose immutable revision supplies the component declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ComponentOwner {
    Source { name: String },
    Installation,
}

/// Installation operations that must have an owner before component selection.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum InstallationInput {
    Namespace,
    NodeBootstrap,
    PublicResources,
    GatewayActivation,
    GpuAllocator,
    GpuPlacement,
}

impl InstallationInput {
    pub fn target(self, profile: &DeploymentProfile) -> Result<super::AtomicTarget> {
        let name = match self {
            Self::Namespace => "installation-namespace",
            Self::NodeBootstrap => "installation-node-bootstrap",
            Self::PublicResources => "installation-public-resources",
            Self::GatewayActivation => "installation-gateway-activation",
            Self::GpuPlacement => "installation-gpu-placement",
            Self::GpuAllocator => {
                let allocator = profile
                    .platform
                    .resolve()?
                    .gpu_scheduling
                    .context("GPU allocator operation has no scheduling configuration")?
                    .allocator
                    .installation;
                return Ok(super::AtomicTarget::HelmRelease {
                    namespace: allocator.namespace,
                    name: allocator.release_name,
                });
            }
        };
        Ok(super::AtomicTarget::ManifestSet { name: name.into() })
    }
}

/// Profile-owned constraints from which the compiler seals exact object inventories.
/// Helm release names refer to complete releases already declared by the source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileComponent {
    pub id: ComponentId,
    pub owner: ComponentOwner,
    pub role: ComponentRole,
    pub dependencies: BTreeSet<ComponentId>,
    pub namespaces: BTreeSet<String>,
    /// Explicit cluster-scoped identities; rendered namespaced objects are sealed
    /// into the lock after the compiler verifies their namespace and unique owner.
    pub cluster_objects: BTreeSet<ObjectIdentity>,
    pub releases: BTreeSet<String>,
    pub installation_inputs: BTreeSet<InstallationInput>,
    pub extension_release: Option<ComponentExtensionRelease>,
}

/// Exact release footprint of an already expanded selection. Installation-only
/// components need no source checkout; their files belong to the profile snapshot.
pub fn selected_source_releases(
    profile: &DeploymentProfile,
    selected: &BTreeSet<ComponentId>,
) -> Result<BTreeMap<String, BTreeSet<String>>> {
    validate_profile_components(profile, &profile.components)?;
    ensure!(
        !selected.is_empty(),
        "source resolution requires exact component IDs"
    );
    let mut sources = BTreeMap::<String, BTreeSet<String>>::new();
    for id in selected {
        let component = profile
            .components
            .iter()
            .find(|component| &component.id == id)
            .with_context(|| format!("unknown selected component {id}"))?;
        ensure!(
            component.dependencies.is_subset(selected),
            "component {id} selection omits an expanded dependency"
        );
        if let ComponentOwner::Source { name } = &component.owner {
            sources
                .entry(name.clone())
                .or_default()
                .extend(component.releases.iter().cloned());
        }
    }
    Ok(sources)
}

/// Checks the entire profile's operation ownership before resolving a selection.
pub fn validate_profile_components(
    profile: &DeploymentProfile,
    components: &[ProfileComponent],
) -> Result<()> {
    ensure!(
        !components.is_empty(),
        "deployment must declare component ownership"
    );
    ensure!(
        profile
            .sources
            .iter()
            .all(|source| source.name != super::INSTALLATION_SOURCE_NAME),
        "source name installation is reserved for installation-owned inputs"
    );
    let sources = profile
        .sources
        .iter()
        .map(|source| (&source.name, source))
        .collect::<BTreeMap<_, _>>();
    let mut ids = BTreeSet::new();
    let mut releases = BTreeMap::new();
    let mut installation_inputs = BTreeMap::new();
    let mut cluster_objects = BTreeMap::new();
    for component in components {
        ensure!(
            ids.insert(&component.id),
            "duplicate component {}",
            component.id
        );
        ensure!(
            !component.namespaces.is_empty(),
            "component {} must declare namespaces",
            component.id
        );
        for namespace in &component.namespaces {
            crate::validate_name("component namespace", namespace)?;
        }
        ensure!(
            !component.releases.is_empty() || !component.installation_inputs.is_empty(),
            "component {} owns no deployment operation",
            component.id
        );
        ensure!(
            (component.role == ComponentRole::Extension) == component.extension_release.is_some(),
            "exact extension release identity is required only for extension components"
        );
        match &component.owner {
            ComponentOwner::Installation => {
                ensure!(
                    component.role == ComponentRole::Installation,
                    "installation-owned component must use the installation role"
                );
                ensure!(
                    component.releases.is_empty(),
                    "installation component cannot own a source Helm release"
                );
            }
            ComponentOwner::Source { name } => {
                let source = sources.get(name).with_context(|| {
                    format!("component {} names unknown source {name}", component.id)
                })?;
                let role = match source.role {
                    DeploymentSourceRole::Platform => ComponentRole::Platform,
                    DeploymentSourceRole::Workload => ComponentRole::Workload,
                    DeploymentSourceRole::Extension => ComponentRole::Extension,
                };
                ensure!(
                    component.role == role,
                    "component role differs from its source owner"
                );
                ensure!(
                    component.installation_inputs.is_empty(),
                    "source component cannot own installation operations"
                );
                ensure!(
                    component.namespaces.contains(&profile.namespace),
                    "source component omits its Helm namespace"
                );
                for release in &component.releases {
                    ensure!(
                        source
                            .releases
                            .iter()
                            .any(|candidate| &candidate.name == release),
                        "component {} names a release outside source {name}",
                        component.id
                    );
                    if let Some(owner) = releases.insert((name, release), &component.id) {
                        anyhow::bail!(
                            "Helm release {release} is owned by both {owner} and {}",
                            component.id
                        );
                    }
                }
            }
        }
        for input in &component.installation_inputs {
            if let Some(owner) = installation_inputs.insert(*input, &component.id) {
                anyhow::bail!(
                    "installation input {input:?} is owned by both {owner} and {}",
                    component.id
                );
            }
        }
        for object in &component.cluster_objects {
            ensure!(
                object.namespace.is_none(),
                "cluster object permission must not contain a namespace"
            );
            super::validation::validate_object(object)?;
            if let Some(owner) = cluster_objects.insert(object, &component.id) {
                anyhow::bail!(
                    "cluster object is owned by both {owner} and {}",
                    component.id
                );
            }
        }
    }
    for source in &profile.sources {
        for release in &source.releases {
            ensure!(
                releases.contains_key(&(&source.name, &release.name)),
                "Helm release {} has no component owner",
                release.name
            );
        }
    }
    let required = required_installation_inputs(profile)?;
    ensure!(
        installation_inputs.keys().copied().collect::<BTreeSet<_>>() == required,
        "component owners must cover exactly the declared installation operations"
    );
    for component in components {
        for dependency in &component.dependencies {
            ensure!(
                ids.contains(dependency),
                "component {} requires missing dependency {dependency}",
                component.id
            );
        }
    }
    let mut remaining = components
        .iter()
        .map(|c| (&c.id, c.dependencies.clone()))
        .collect::<BTreeMap<_, _>>();
    while !remaining.is_empty() {
        let next = remaining
            .iter()
            .find(|(_, deps)| deps.is_empty())
            .map(|(id, _)| (*id).clone())
            .context("component dependency graph contains a cycle")?;
        remaining.remove(&next);
        for deps in remaining.values_mut() {
            deps.remove(&next);
        }
    }
    for component in components {
        let mut ancestors = BTreeSet::new();
        let mut pending = component.dependencies.iter().cloned().collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if ancestors.insert(id.clone()) {
                let parent = components
                    .iter()
                    .find(|parent| parent.id == id)
                    .context("component prerequisite is missing")?;
                pending.extend(parent.dependencies.iter().cloned());
            }
        }
        let mut prerequisites = BTreeSet::new();
        if matches!(component.owner, ComponentOwner::Source { .. })
            || component.installation_inputs.iter().any(|input| {
                matches!(
                    input,
                    InstallationInput::PublicResources
                        | InstallationInput::GatewayActivation
                        | InstallationInput::GpuAllocator
                        | InstallationInput::GpuPlacement
                )
            })
        {
            prerequisites.insert(InstallationInput::Namespace);
        }
        if component
            .installation_inputs
            .contains(&InstallationInput::GpuPlacement)
        {
            prerequisites.insert(InstallationInput::GpuAllocator);
        }
        if component
            .installation_inputs
            .contains(&InstallationInput::GpuAllocator)
            && installation_inputs.contains_key(&InstallationInput::NodeBootstrap)
        {
            prerequisites.insert(InstallationInput::NodeBootstrap);
        }
        for operation in prerequisites {
            let owner = installation_inputs
                .get(&operation)
                .context("installation prerequisite has no owner")?;
            ensure!(
                **owner == component.id || ancestors.contains(*owner),
                "component {} must depend on owner {} of prerequisite {operation:?}",
                component.id,
                owner
            );
        }
    }
    Ok(())
}

pub fn required_installation_inputs(
    profile: &DeploymentProfile,
) -> Result<BTreeSet<InstallationInput>> {
    let mut inputs = BTreeSet::from([InstallationInput::Namespace]);
    if profile
        .kubernetes
        .local_cluster
        .as_ref()
        .is_some_and(|cluster| !cluster.node_bootstrap_manifests.is_empty())
    {
        inputs.insert(InstallationInput::NodeBootstrap);
    }
    if !profile.resources.manifests.is_empty() || !profile.resources.config_maps.is_empty() {
        inputs.insert(InstallationInput::PublicResources);
    }
    if profile.gateway_activation.is_some() {
        inputs.insert(InstallationInput::GatewayActivation);
    }
    if profile.platform.resolve()?.gpu_scheduling.is_some() {
        inputs.insert(InstallationInput::GpuAllocator);
        inputs.insert(InstallationInput::GpuPlacement);
    }
    Ok(inputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_cannot_shadow_installation_input_provenance() {
        let mut profile: DeploymentProfile = serde_json::from_str(include_str!(
            "../../../../showcase/sumo/deploy/deployment.json"
        ))
        .unwrap();
        validate_profile_components(&profile, &profile.components).unwrap();
        let previous = std::mem::replace(
            &mut profile.sources[0].name,
            super::super::INSTALLATION_SOURCE_NAME.into(),
        );
        for component in &mut profile.components {
            if let ComponentOwner::Source { name } = &mut component.owner
                && name == &previous
            {
                *name = super::super::INSTALLATION_SOURCE_NAME.into();
            }
        }
        assert!(
            validate_profile_components(&profile, &profile.components)
                .unwrap_err()
                .to_string()
                .contains("reserved")
        );
    }

    #[test]
    fn source_selection_must_include_its_namespace_owner() {
        let mut profile: DeploymentProfile = serde_json::from_str(include_str!(
            "../../../../showcase/sumo/deploy/deployment.json"
        ))
        .unwrap();
        let source = profile
            .components
            .iter_mut()
            .find(|component| matches!(component.owner, ComponentOwner::Source { .. }))
            .unwrap();
        source.dependencies.clear();
        assert!(
            validate_profile_components(&profile, &profile.components)
                .unwrap_err()
                .to_string()
                .contains("prerequisite Namespace")
        );
    }

    #[test]
    fn source_footprint_keeps_whole_catalog_validation_and_selects_exact_releases() {
        let mut profile: DeploymentProfile = serde_json::from_str(include_str!(
            "../../../../showcase/sumo/deploy/deployment.json"
        ))
        .unwrap();
        let selected = profile
            .components
            .iter()
            .map(|component| component.id.clone())
            .collect::<BTreeSet<_>>();
        let mut extra = profile
            .components
            .iter()
            .find(|component| matches!(component.owner, ComponentOwner::Source { .. }))
            .unwrap()
            .clone();
        extra.id = "unselected-release".to_owned().try_into().unwrap();
        extra.releases = BTreeSet::from(["unselected-release".into()]);
        let ComponentOwner::Source { name } = &extra.owner else {
            unreachable!()
        };
        let owner_name = name.clone();
        let source = profile
            .sources
            .iter_mut()
            .find(|source| source.name == owner_name)
            .unwrap();
        source.releases.push(crate::ReleaseSpec {
            name: "unselected-release".into(),
            chart: "unselected-chart-not-present".into(),
            ..source.releases[0].clone()
        });
        profile.components.push(extra);
        let footprint = selected_source_releases(&profile, &selected).unwrap();
        assert!(!footprint[&owner_name].contains("unselected-release"));

        assert!(selected_source_releases(&profile, &BTreeSet::new()).is_err());
        let mut unknown = selected.clone();
        unknown.insert("unknown".to_owned().try_into().unwrap());
        assert!(selected_source_releases(&profile, &unknown).is_err());
        let mut incomplete = selected.clone();
        let namespace_owner = profile
            .components
            .iter()
            .find(|component| {
                component
                    .installation_inputs
                    .contains(&InstallationInput::Namespace)
            })
            .unwrap();
        incomplete.remove(&namespace_owner.id);
        assert!(
            selected_source_releases(&profile, &incomplete)
                .unwrap_err()
                .to_string()
                .contains("expanded dependency")
        );

        profile.components.last_mut().unwrap().owner = ComponentOwner::Source {
            name: "unknown".into(),
        };
        assert!(
            selected_source_releases(&profile, &selected)
                .unwrap_err()
                .to_string()
                .contains("unknown source")
        );
    }
}
