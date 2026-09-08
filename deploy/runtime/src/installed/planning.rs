//! Convert checked cluster state into executable atomic-unit decisions.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use veoveo_deploy_contract::components::*;

use super::{InstallOutcome, InstalledState};
use crate::{
    compile::{CompiledComponent, CompiledUnit, objects::object_digest},
    ownership::{
        OwnershipObservation, read_objects, validate_live_ownership, validate_manager_value,
    },
};

impl InstalledState {
    pub(crate) fn plan(
        &self,
        catalog: &[LockedComponent],
        requested: &BTreeSet<ComponentId>,
        compiled: &[CompiledComponent],
        invalidated: &BTreeSet<ObjectIdentity>,
    ) -> Result<ComponentMutationPlan> {
        let ownership = validate_live_ownership(&self.context, catalog, compiled)?;
        let mut observed = Vec::new();
        let mut prepared = Vec::new();
        for component in compiled {
            for unit in &component.units {
                observed.push(self.observe(component, unit, &ownership, invalidated)?);
                prepared.push(unit.prepared.clone());
            }
        }
        component_mutation_plan(catalog, requested, &prepared, &observed)
    }

    fn observe(
        &self,
        component: &CompiledComponent,
        unit: &CompiledUnit,
        ownership: &OwnershipObservation,
        invalidated: &BTreeSet<ObjectIdentity>,
    ) -> Result<ObservedAtomicUnit> {
        let receipt = self.receipt(component, unit)?;
        let invalidated = unit
            .prepared
            .objects
            .iter()
            .any(|object| invalidated.contains(&object.identity));
        if !invalidated
            && let Some(receipt) = &receipt
            && self.reusable(component, unit, receipt)?
        {
            return Ok(ObservedAtomicUnit {
                component: component.locked.declaration.id.clone(),
                target: unit.prepared.target.clone(),
                state: ObservedUnitState::Present {
                    digest: receipt.unit.digest.clone(),
                    content_digest: receipt.unit.content_digest.clone(),
                    objects: receipt.unit.objects.clone(),
                },
            });
        }
        let mut inventory = ownership
            .historical
            .get(&unit.prepared.target)
            .cloned()
            .unwrap_or_default();
        let mut live = unit
            .prepared
            .objects
            .iter()
            .filter_map(|object| {
                ownership
                    .live
                    .get(&object.identity)
                    .map(|value| (object.identity.clone(), value.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        if matches!(unit.prepared.target, AtomicTarget::ManifestSet { .. })
            && let Some(receipt) = &receipt
        {
            let previous = receipt
                .unit
                .objects
                .iter()
                .map(|object| object.identity.clone())
                .collect();
            // Include old raw objects that disappeared from the new manifest set.
            // The pure planner rejects their retirement instead of abandoning them.
            live.extend(read_objects(&self.context, &previous, None)?);
        }
        for identity in inventory.keys() {
            if let Some(value) = ownership.live.get(identity) {
                live.insert(identity.clone(), value.clone());
            }
        }
        for (identity, value) in live {
            validate_manager_value(&unit.prepared.target, &value)?;
            inventory.insert(
                identity.clone(),
                RenderedObject {
                    identity,
                    digest: object_digest(&value)?,
                },
            );
        }
        let state =
            if inventory.is_empty() && !ownership.releases.contains_key(&unit.prepared.target) {
                ObservedUnitState::Absent
            } else {
                ObservedUnitState::RequiresApply {
                    objects: inventory.into_values().collect(),
                }
            };
        Ok(ObservedAtomicUnit {
            component: component.locked.declaration.id.clone(),
            target: unit.prepared.target.clone(),
            state,
        })
    }

    pub(crate) fn apply_planned(
        &self,
        component: &CompiledComponent,
        unit: &CompiledUnit,
        plan: &ComponentMutationPlan,
    ) -> Result<InstallOutcome> {
        let mutation = plan
            .mutations
            .iter()
            .find(|mutation| {
                mutation.component == component.locked.declaration.id
                    && mutation.target == unit.prepared.target
            })
            .context("installation target has no preflight mutation plan")?;
        let locked = component
            .locked
            .units
            .iter()
            .find(|locked| locked.target == unit.prepared.target)
            .context("installation target has no locked unit")?;
        ensure!(
            mutation.source == component.locked.declaration.source
                && mutation.configuration == component.locked.declaration.configuration
                && mutation.digest == locked.digest
                && mutation.content_digest == locked.content_digest
                && mutation.objects == unit.prepared.objects,
            "installation input changed after mutation planning"
        );
        match mutation.verb {
            ComponentMutationVerb::Unchanged => {
                let receipt = self
                    .receipt(component, unit)?
                    .context("unchanged installation lost its receipt")?;
                ensure!(
                    receipt.unit.content_digest == locked.content_digest
                        && self.reusable(component, unit, &receipt)?,
                    "unchanged installation drifted after preflight; prepare a new plan"
                );
                Ok(InstallOutcome::Reused)
            }
            ComponentMutationVerb::HelmUpgradeInstall
                if matches!(unit.prepared.target, AtomicTarget::HelmRelease { .. }) =>
            {
                self.apply(component, unit)
            }
            ComponentMutationVerb::ApplyExplicitObjects
                if matches!(unit.prepared.target, AtomicTarget::ManifestSet { .. }) =>
            {
                self.apply(component, unit)
            }
            _ => anyhow::bail!("planned mutation verb disagrees with the atomic target"),
        }
    }
}
