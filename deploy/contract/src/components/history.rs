//! Historical Helm objects remain part of an upgrade's deletion boundary.
use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};

use super::{types::*, validate_component_catalog, validation::validate_object};

/// Checks a complete stored Helm manifest against every current owner, including
/// unselected and reserved identities. The runtime verifies its Helm provenance.
pub fn validate_helm_inventory(
    catalog: &[LockedComponent],
    component: &ComponentId,
    target: &AtomicTarget,
    objects: &[ObjectIdentity],
) -> Result<()> {
    validate_component_catalog(catalog)?;
    ensure!(
        matches!(target, AtomicTarget::HelmRelease { .. }),
        "historical Helm inventory requires an exact release"
    );
    let owner = catalog
        .iter()
        .find(|owner| &owner.declaration.id == component)
        .context("historical Helm owner is outside the catalog")?;
    ensure!(
        owner.declaration.targets.contains(target),
        "historical Helm target is outside its component"
    );
    let mut unique = BTreeSet::new();
    for object in objects {
        ensure!(
            unique.insert(object),
            "historical Helm inventory contains a duplicate object"
        );
        validate_historical_object(catalog, owner, target, object)?;
    }
    Ok(())
}

pub(super) fn validate_historical_object(
    catalog: &[LockedComponent],
    owner: &LockedComponent,
    target: &AtomicTarget,
    object: &ObjectIdentity,
) -> Result<()> {
    validate_object(object)?;
    match &object.namespace {
        Some(namespace) => ensure!(
            owner.declaration.namespaces.contains(namespace),
            "historical Helm object uses an undeclared namespace"
        ),
        None => ensure!(
            owner.declaration.permitted_objects.contains(object),
            "historical Helm object lacks explicit cluster permission"
        ),
    }
    for candidate in catalog {
        ensure!(
            candidate.declaration.id == owner.declaration.id
                || !candidate.declaration.permitted_objects.contains(object),
            "historical Helm object is outside component {} ownership",
            owner.declaration.id
        );
        for unit in &candidate.units {
            ensure!(
                &unit.target == target
                    || !unit
                        .objects
                        .iter()
                        .any(|current| &current.identity == object),
                "historical Helm object cannot transfer between atomic targets"
            );
        }
    }
    Ok(())
}
