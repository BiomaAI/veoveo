use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};

use super::{
    COMPONENT_MUTATION_PLAN_SCHEMA, atomic_unit_content_digest, atomic_unit_digest,
    types::*,
    validate_component_catalog,
    validation::{dependency_order, validate_owned_object},
};

/// Expands exact IDs in deterministic dependency order before a renderer runs.
/// The complete locked catalog is validated even when one component is requested.
pub fn select_components(
    catalog: &[LockedComponent],
    requested: &BTreeSet<ComponentId>,
) -> Result<Vec<ComponentId>> {
    validate_component_catalog(catalog)?;
    expand(catalog, requested)
}

fn expand(
    catalog: &[LockedComponent],
    requested: &BTreeSet<ComponentId>,
) -> Result<Vec<ComponentId>> {
    ensure!(
        !requested.is_empty(),
        "at least one exact component ID is required"
    );
    let by_id = catalog
        .iter()
        .map(|component| (&component.declaration.id, component))
        .collect::<BTreeMap<_, _>>();
    let mut selected = requested.clone();
    let mut pending = requested.iter().cloned().collect::<Vec<_>>();
    while let Some(id) = pending.pop() {
        let component = by_id
            .get(&id)
            .with_context(|| format!("unknown component {id}"))?;
        for dependency in &component.declaration.dependencies {
            if selected.insert(dependency.clone()) {
                pending.push(dependency.clone());
            }
        }
    }
    Ok(dependency_order(catalog)?
        .into_iter()
        .filter(|id| selected.contains(id))
        .collect())
}

/// Produces the complete allowed mutation plan before execution can start.
///
/// Every expanded target needs a complete render and a verified observation, including
/// an explicit observation of absence. Unselected targets must never be rendered.
/// The caller must preserve the checked bytes between preflight and execution and
/// prevent concurrent ownership changes. This result alone is not write evidence.
pub fn component_mutation_plan(
    catalog: &[LockedComponent],
    requested: &BTreeSet<ComponentId>,
    prepared: &[PreparedAtomicUnit],
    observed: &[ObservedAtomicUnit],
) -> Result<ComponentMutationPlan> {
    validate_component_catalog(catalog)?;
    let expanded = expand(catalog, requested)?;
    let selected = expanded.iter().collect::<BTreeSet<_>>();
    let by_id = catalog
        .iter()
        .map(|component| (&component.declaration.id, component))
        .collect::<BTreeMap<_, _>>();
    let mut prepared_by_target = BTreeMap::new();
    for unit in prepared {
        ensure!(
            selected.contains(&unit.component),
            "unselected component was rendered"
        );
        ensure!(
            prepared_by_target
                .insert((&unit.component, &unit.target), unit)
                .is_none(),
            "duplicate prepared atomic target"
        );
    }
    let mut observed_by_target = BTreeMap::new();
    let mut observed_objects = BTreeSet::new();
    let desired_targets = catalog
        .iter()
        .flat_map(|component| {
            component.units.iter().flat_map(|unit| {
                unit.objects
                    .iter()
                    .map(move |object| (&object.identity, &unit.target))
            })
        })
        .collect::<BTreeMap<_, _>>();
    for unit in observed {
        let component = by_id
            .get(&unit.component)
            .context("observed owner is not in locked catalog")?;
        ensure!(
            component.declaration.targets.contains(&unit.target),
            "observed target is outside component ownership"
        );
        ensure!(
            observed_by_target
                .insert((&unit.component, &unit.target), &unit.state)
                .is_none(),
            "duplicate observed atomic target"
        );
        if let ObservedUnitState::Present { objects, .. } = &unit.state {
            ensure!(
                !objects.is_empty(),
                "observed atomic target has no explicit inventory"
            );
            for object in objects {
                // Helm can delete an object that disappeared from the desired chart.
                // Such an object must still belong to this owner, never another component.
                validate_owned_object(&component.declaration, &object.identity)?;
                if let Some(desired) = desired_targets.get(&object.identity) {
                    ensure!(
                        **desired == unit.target,
                        "object cannot transfer between atomic targets during an update"
                    );
                }
                ensure!(
                    observed_objects.insert(&object.identity),
                    "observed object occurs in multiple atomic targets"
                );
            }
        }
    }
    let mut mutations = Vec::new();
    let mut expected_targets = BTreeSet::new();
    for id in &expanded {
        let component = by_id[id];
        // Catalog ordering must not affect the execution order within an owner.
        let mut units = component.units.iter().collect::<Vec<_>>();
        units.sort_by(|left, right| left.target.cmp(&right.target));
        for locked in units {
            let key = (id, &locked.target);
            expected_targets.insert(key);
            let prepared = prepared_by_target.get(&key).with_context(|| {
                format!("component {id} is missing a complete atomic target render")
            })?;
            let state = observed_by_target.get(&key).with_context(|| {
                format!("component {id} is missing a verified atomic target observation")
            })?;
            ensure!(
                prepared.inputs == locked.inputs,
                "prepared inputs differ from locked atomic unit"
            );
            let digest = atomic_unit_digest(&component.declaration, prepared)?;
            ensure!(
                digest == locked.digest,
                "prepared rendering differs from locked atomic unit"
            );
            let content_digest = atomic_unit_content_digest(&component.declaration, prepared)?;
            ensure!(
                content_digest == locked.content_digest,
                "prepared contents differ from locked atomic unit"
            );
            let unchanged = match state {
                ObservedUnitState::Absent => false,
                ObservedUnitState::Present {
                    digest: installed_digest,
                    content_digest: installed_content_digest,
                    objects,
                } => {
                    ensure!(
                        installed_digest != &digest || installed_content_digest == &content_digest,
                        "installed provenance matches but content digest differs"
                    );
                    if installed_content_digest == &content_digest {
                        ensure!(
                            same_objects(objects, &locked.objects),
                            "installed digest matches but object inventory has drifted"
                        );
                        true
                    } else {
                        false
                    }
                }
            };
            let verb = if unchanged {
                ComponentMutationVerb::Unchanged
            } else {
                match locked.target {
                    AtomicTarget::HelmRelease { .. } => ComponentMutationVerb::HelmUpgradeInstall,
                    AtomicTarget::ManifestSet { .. } => ComponentMutationVerb::ApplyExplicitObjects,
                }
            };
            let desired_objects = prepared
                .objects
                .iter()
                .map(|object| &object.identity)
                .collect::<BTreeSet<_>>();
            let retired_objects = match state {
                ObservedUnitState::Absent => Vec::new(),
                ObservedUnitState::Present { objects, .. } => objects
                    .iter()
                    .filter(|object| !desired_objects.contains(&object.identity))
                    .cloned()
                    .collect::<Vec<_>>(),
            };
            ensure!(
                retired_objects.is_empty()
                    || matches!(locked.target, AtomicTarget::HelmRelease { .. }),
                "raw apply cannot remove retired objects; an explicit removal migration is required"
            );
            mutations.push(ComponentMutation {
                component: id.clone(),
                source: component.declaration.source.clone(),
                target: locked.target.clone(),
                digest,
                content_digest,
                objects: prepared.objects.clone(),
                retired_objects,
                verb,
            });
        }
    }
    ensure!(
        prepared_by_target.keys().copied().collect::<BTreeSet<_>>() == expected_targets,
        "prepared atomic targets do not exactly cover expanded components"
    );
    let unselected_objects = catalog
        .iter()
        .filter(|component| !selected.contains(&component.declaration.id))
        .flat_map(|component| component.declaration.permitted_objects.iter().cloned())
        .collect();
    Ok(ComponentMutationPlan {
        schema_version: COMPONENT_MUTATION_PLAN_SCHEMA.into(),
        requested: requested.clone(),
        expanded,
        mutations,
        unselected_objects,
    })
}

fn same_objects(left: &[RenderedObject], right: &[RenderedObject]) -> bool {
    let as_map = |objects: &[RenderedObject]| {
        objects
            .iter()
            .map(|object| (object.identity.clone(), object.digest.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    as_map(left) == as_map(right)
}
