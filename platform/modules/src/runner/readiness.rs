//! Read-only runtime capability for a compiled prerequisite closure.
use super::{ExecutionLimits, RunnerError, executor::transaction, prepare};
use crate::*;
use std::collections::BTreeSet;
use surrealdb::{Connection, Surreal};

/// Unlike full installer status, this capability intentionally inspects only its
/// declared owner closure. It exposes no initialization or mutation operations.
pub struct RuntimePrerequisites {
    registry: ModuleRegistry,
}
impl RuntimePrerequisites {
    pub fn new(
        registry: &ModuleRegistry,
        plan: &ModulePlanDocument,
        required: &[ModuleName],
    ) -> Result<Self, RunnerError> {
        fn include(
            registry: &ModuleRegistry,
            name: &ModuleName,
            names: &mut BTreeSet<ModuleName>,
        ) -> Result<(), RunnerError> {
            if !names.insert(name.clone()) {
                return Ok(());
            }
            let module = registry.module(name).ok_or_else(|| {
                RunnerError::new(format!("runtime prerequisite {name} is not compiled"))
            })?;
            for dependency in module.requires() {
                include(registry, dependency.module(), names)?;
            }
            Ok(())
        }
        if required.is_empty() {
            return Err(RunnerError::new("runtime prerequisites cannot be empty"));
        }
        let mut names = BTreeSet::new();
        for name in required {
            include(registry, name, &mut names)?;
        }
        let registry = ModuleRegistry::new(
            registry
                .modules()
                .iter()
                .filter(|module| names.contains(module.name()))
                .cloned()
                .collect(),
        )
        .map_err(|error| RunnerError::new(error.to_string()))?;
        let enabled: Vec<_> = registry
            .modules()
            .iter()
            .filter(|module| module.layer() == ModuleLayer::Optional)
            .map(|module| module.name().clone())
            .collect();
        let selection = ModuleSelectionDocument::new(
            enabled,
            plan.generation(),
            plan.credential_revision().clone(),
        )
        .map_err(|error| RunnerError::new(error.to_string()))?;
        let expected = ModulePlanDocument::generate(
            &registry,
            &selection,
            plan.composition().clone(),
            Vec::new(),
        )
        .map_err(|error| RunnerError::new(error.to_string()))?;
        for descriptor in expected.lanes() {
            if plan
                .lanes()
                .iter()
                .find(|lane| lane.module == descriptor.module)
                != Some(descriptor)
            {
                return Err(RunnerError::new(format!(
                    "selected runtime prerequisite {} differs from compiled owner declaration",
                    descriptor.module
                )));
            }
        }
        // Reuse complete SQL/ownership admission without granting the caller execution.
        prepare(
            registry
                .select(selection.enabled().to_vec())
                .map_err(|error| RunnerError::new(error.to_string()))?,
        )?;
        Ok(Self { registry })
    }
    pub async fn require<C: Connection>(
        &self,
        db: &Surreal<C>,
        key: &PreparationKey,
        limits: ExecutionLimits,
    ) -> Result<(), RunnerError> {
        if !super::executor::infrastructure_state(db, limits).await? {
            return Err(RunnerError::new(
                "runtime installation bookkeeping is absent",
            ));
        }
        let enabled = self
            .registry
            .modules()
            .iter()
            .filter(|module| module.layer() == ModuleLayer::Optional)
            .map(|module| module.name().clone())
            .collect();
        let prepared = prepare(
            self.registry
                .select(enabled)
                .map_err(|error| RunnerError::new(error.to_string()))?,
        )?;
        let modules = self
            .registry
            .modules()
            .iter()
            .map(|module| module.name().as_str().to_owned())
            .collect();
        match transaction::execute(
            db,
            transaction::Operation::Prerequisites {
                modules,
                key: key.clone(),
            },
            limits,
        )
        .await?
        {
            transaction::Output::History(headers, applied) => {
                let status = prepared.validate_history(&headers, &applied)?;
                if !status.is_current() {
                    return Err(RunnerError::new(
                        "selected runtime prerequisite migrations are incomplete",
                    ));
                }
                Ok(())
            }
            _ => unreachable!("prerequisite inspection returns history"),
        }
    }
}
