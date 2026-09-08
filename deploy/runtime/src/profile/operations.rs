//! Record actual unit outcomes while enforcing complete planned execution.
use crate::{
    compile::{CompiledComponent, CompiledUnit},
    installed::{InstallOutcome, InstalledState},
};
use anyhow::{Result, ensure};
use std::collections::BTreeSet;
use veoveo_deploy_contract::components::{
    ComponentMutationPlan, UnitExecution, UnitExecutionOutcome,
};

pub(super) struct Operations<'a> {
    installed: &'a InstalledState,
    plan: &'a ComponentMutationPlan,
    performed: Vec<UnitExecution>,
}

impl<'a> Operations<'a> {
    pub fn new(installed: &'a InstalledState, plan: &'a ComponentMutationPlan) -> Self {
        Self {
            installed,
            plan,
            performed: Vec::new(),
        }
    }

    pub fn apply(&mut self, component: &CompiledComponent, unit: &CompiledUnit) -> Result<()> {
        let outcome = match self.installed.apply_planned(component, unit, self.plan)? {
            InstallOutcome::Reused => UnitExecutionOutcome::Reused,
            InstallOutcome::Applied { .. } => UnitExecutionOutcome::Applied,
        };
        self.record(component, unit, outcome)
    }

    /// Called only after the immutable claim helper verifies its live spec and UID.
    pub fn reuse_claim(
        &mut self,
        component: &CompiledComponent,
        unit: &CompiledUnit,
    ) -> Result<()> {
        ensure!(
            unit.installation_input
                == Some(veoveo_deploy_contract::components::InstallationInput::GpuPlacement),
            "immutable-claim reuse requires the compiled claim unit"
        );
        self.record(component, unit, UnitExecutionOutcome::Reused)
    }

    fn record(
        &mut self,
        component: &CompiledComponent,
        unit: &CompiledUnit,
        outcome: UnitExecutionOutcome,
    ) -> Result<()> {
        ensure!(
            self.plan.mutations.iter().any(|mutation| mutation.component
                == component.locked.declaration.id
                && mutation.target == unit.prepared.target),
            "executed unit is outside the mutation plan"
        );
        self.performed.push(UnitExecution {
            component: component.locked.declaration.id.clone(),
            target: unit.prepared.target.clone(),
            outcome,
        });
        Ok(())
    }

    pub fn finish(self) -> Result<Vec<UnitExecution>> {
        let actual = self
            .performed
            .iter()
            .map(|operation| (&operation.component, &operation.target))
            .collect::<BTreeSet<_>>();
        let expected = self
            .plan
            .mutations
            .iter()
            .map(|mutation| (&mutation.component, &mutation.target))
            .collect::<BTreeSet<_>>();
        ensure!(
            actual.len() == self.performed.len() && actual == expected,
            "executed units differ from the complete planned selection"
        );
        Ok(self.performed)
    }
}
