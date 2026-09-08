//! Verified reuse at the existing full-profile execution boundary.
use std::{collections::BTreeSet, path::Path};

use anyhow::{Context, Result, ensure};
use veoveo_deploy_contract::components::*;

use crate::{
    compile::{CompiledComponent, CompiledUnit},
    helm_state::snapshot::{HelmSnapshotEvidence, ReleaseSnapshot, same_inventory},
    ownership::read_objects,
    process::kubectl_apply_value,
};

mod normalize;
mod objects;
mod planning;
mod store;
use store::ReceiptStore;

pub(crate) struct InstalledState {
    context: String,
    store: ReceiptStore,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InstallOutcome {
    Reused,
    Applied { recorded: bool },
}

impl InstalledState {
    pub(crate) fn open(
        repository: &Path,
        context: &str,
        compiled: &[CompiledComponent],
    ) -> Result<Self> {
        let state = Self {
            context: context.into(),
            store: ReceiptStore::open(repository, context)?,
        };
        // Decode every relevant local receipt before any installation write.
        for component in compiled {
            for unit in &component.units {
                state.receipt(component, unit)?;
            }
        }
        Ok(state)
    }

    fn receipt(
        &self,
        component: &CompiledComponent,
        unit: &CompiledUnit,
    ) -> Result<Option<InstalledUnitReceipt>> {
        let receipt = self.store.load(&unit.prepared.target)?;
        if let Some(receipt) = &receipt {
            ensure!(
                receipt.component.id == component.locked.declaration.id,
                "installed target receipt belongs to another component"
            );
        }
        Ok(receipt)
    }

    fn helm(
        &self,
        component: &CompiledComponent,
        unit: &CompiledUnit,
    ) -> Result<Option<HelmSnapshotEvidence>> {
        let AtomicTarget::HelmRelease { namespace, name } = &unit.prepared.target else {
            return Ok(None);
        };
        let Some(snapshot) = ReleaseSnapshot::read(&self.context, namespace, name)? else {
            return Ok(None);
        };
        if !snapshot.deployed() {
            return Ok(None);
        }
        Ok(Some(snapshot.evidence(&component.locked.declaration)?))
    }

    pub(crate) fn apply(
        &self,
        component: &CompiledComponent,
        unit: &CompiledUnit,
    ) -> Result<InstallOutcome> {
        self.run(component, unit, || {
            match (&unit.prepared.target, &unit.helm) {
                (AtomicTarget::HelmRelease { namespace, name }, Some(helm)) => {
                    helm.install(&self.context, namespace, name)
                }
                (AtomicTarget::ManifestSet { .. }, None) => {
                    for object in &unit.objects {
                        kubectl_apply_value(&self.context, object)?;
                    }
                    Ok(())
                }
                _ => anyhow::bail!("prepared operation disagrees with atomic target"),
            }
        })
    }

    fn run(
        &self,
        component: &CompiledComponent,
        unit: &CompiledUnit,
        apply: impl FnOnce() -> Result<()>,
    ) -> Result<InstallOutcome> {
        let locked = component
            .locked
            .units
            .iter()
            .find(|locked| locked.target == unit.prepared.target)
            .context("compiled operation has no locked unit")?;
        ensure!(
            atomic_unit_digest(&component.locked.declaration, &unit.prepared)? == locked.digest,
            "compiled operation differs from locked unit"
        );
        ensure!(
            unit.objects.len() == unit.prepared.objects.len(),
            "compiled object inventory is incomplete"
        );
        if let Some(receipt) = self.receipt(component, unit)?
            && receipt.unit.content_digest == locked.content_digest
            && self.reusable(component, unit, &receipt)?
        {
            println!(
                "Reusing verified deployment unit {} {:?}",
                component.locked.declaration.id, locked.target
            );
            // Preserve the provenance of the operation that actually installed it.
            return Ok(InstallOutcome::Reused);
        }
        // Failure or interruption must never leave the previous receipt reusable.
        self.store.remove(&locked.target)?;
        apply()?;
        let recorded = self.record(component, unit, locked)?;
        if !recorded {
            println!(
                "Deployment unit {} applied; live state is not eligible for reuse",
                component.locked.declaration.id
            );
        }
        Ok(InstallOutcome::Applied { recorded })
    }

    fn reusable(
        &self,
        component: &CompiledComponent,
        unit: &CompiledUnit,
        receipt: &InstalledUnitReceipt,
    ) -> Result<bool> {
        let helm = self.helm(component, unit)?;
        if helm.as_ref().map(|helm| &helm.revision) != receipt.helm.as_ref() {
            return Ok(false);
        }
        let completed = helm
            .as_ref()
            .map(|helm| helm.completed_hooks.clone())
            .unwrap_or_default();
        let expected = receipt
            .objects
            .iter()
            .map(|object| object.identity.clone())
            .collect();
        let live = read_objects(&self.context, &expected, None)?;
        for object in live.values() {
            crate::ownership::validate_manager_value(&unit.prepared.target, object)?;
        }
        if !objects::unchanged(&receipt.objects, &live, &completed)? {
            return Ok(false);
        }
        // A release transition while reading its objects invalidates the observation.
        Ok(self
            .helm(component, unit)?
            .as_ref()
            .map(|helm| &helm.revision)
            == receipt.helm.as_ref())
    }

    fn record(
        &self,
        component: &CompiledComponent,
        unit: &CompiledUnit,
        locked: &LockedAtomicUnit,
    ) -> Result<bool> {
        let helm = self.helm(component, unit)?;
        if matches!(locked.target, AtomicTarget::HelmRelease { .. })
            && helm
                .as_ref()
                .is_none_or(|helm| !same_inventory(&helm.objects, &locked.objects))
        {
            return Ok(false);
        }
        let completed = helm
            .as_ref()
            .map(|helm| helm.completed_hooks.clone())
            .unwrap_or_default();
        let expected = locked
            .objects
            .iter()
            .map(|object| object.identity.clone())
            .collect::<BTreeSet<_>>();
        let live = read_objects(&self.context, &expected, None)?;
        let Some(objects) = objects::capture(unit, &live, &completed, |desired| {
            normalize::project(&self.context, &unit.prepared.target, desired)
        })?
        else {
            return Ok(false);
        };
        if self
            .helm(component, unit)?
            .as_ref()
            .map(|helm| &helm.revision)
            != helm.as_ref().map(|helm| &helm.revision)
        {
            return Ok(false);
        }
        self.store.save(&InstalledUnitReceipt {
            schema_version: INSTALLED_UNIT_SCHEMA.into(),
            cluster_uid: self.store.cluster_uid.clone(),
            component: component.locked.declaration.clone(),
            unit: locked.clone(),
            helm: helm.map(|helm| helm.revision),
            objects,
        })?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
