//! Complete compiled ownership catalog and enabled execution selection.
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct ModuleRegistry {
    modules: Vec<ModuleSetup>,
    indices: BTreeMap<ModuleName, usize>,
    order: Vec<usize>,
}
impl ModuleRegistry {
    pub fn new(modules: Vec<ModuleSetup>) -> Result<Self, DeclarationError> {
        if modules.len() > 256 {
            return Err(DeclarationError::new("module catalog exceeds 256 modules"));
        }
        let mut indices = BTreeMap::new();
        for (index, module) in modules.iter().enumerate() {
            if indices.insert(module.name().clone(), index).is_some() {
                return Err(DeclarationError::new("duplicate module name"));
            }
            for earlier in &modules[..index] {
                if module.ownership().iter().any(|claim| {
                    earlier
                        .ownership()
                        .iter()
                        .any(|other| claim.overlaps(other))
                }) {
                    return Err(DeclarationError::new(format!(
                        "ownership overlap between {} and {}",
                        earlier.name(),
                        module.name()
                    )));
                }
            }
            for table in [LANE_TABLE, MIGRATION_TABLE] {
                if module
                    .ownership()
                    .iter()
                    .any(|claim| claim.matches(ObjectKind::Table, table))
                    && (module.name().as_str() != "store" || module.layer() != ModuleLayer::Kernel)
                {
                    return Err(DeclarationError::new(
                        "lane history tables belong to the Store kernel",
                    ));
                }
            }
        }
        for module in &modules {
            for requirement in module.requires() {
                let required = indices
                    .get(requirement.module())
                    .map(|i| &modules[*i])
                    .ok_or_else(|| {
                        DeclarationError::new(format!(
                            "unknown required module {}",
                            requirement.module()
                        ))
                    })?;
                if module.layer() == ModuleLayer::Kernel && required.layer() != ModuleLayer::Kernel
                {
                    return Err(DeclarationError::new(
                        "kernel module cannot require an optional module",
                    ));
                }
                check_minimum(requirement, required)?;
            }
        }
        let mut registry = Self {
            modules,
            indices,
            order: Vec::new(),
        };
        let mut visited = BTreeSet::new();
        let mut active = BTreeSet::new();
        for index in 0..registry.modules.len() {
            registry.visit(index, &mut visited, &mut active)?;
        }
        for module in &registry.modules {
            for migration in module.lane().migrations() {
                for requirement in migration.requires() {
                    if !registry.depends_on(module.name(), requirement.module()) {
                        return Err(DeclarationError::new(
                            "migration prerequisite outside declared dependency graph",
                        ));
                    }
                    check_minimum(
                        requirement,
                        registry
                            .module(requirement.module())
                            .expect("validated dependency"),
                    )?;
                }
            }
        }
        Ok(registry)
    }
    fn visit(
        &mut self,
        index: usize,
        visited: &mut BTreeSet<usize>,
        active: &mut BTreeSet<usize>,
    ) -> Result<(), DeclarationError> {
        if visited.contains(&index) {
            return Ok(());
        }
        if !active.insert(index) {
            return Err(DeclarationError::new("module dependency cycle"));
        }
        let dependencies: Vec<_> = self.modules[index]
            .requires()
            .iter()
            .map(|r| self.indices[r.module()])
            .collect();
        for dependency in dependencies {
            self.visit(dependency, visited, active)?;
        }
        active.remove(&index);
        visited.insert(index);
        self.order.push(index);
        Ok(())
    }
    pub fn modules(&self) -> &[ModuleSetup] {
        &self.modules
    }
    pub fn module(&self, name: &ModuleName) -> Option<&ModuleSetup> {
        self.indices.get(name).map(|i| &self.modules[*i])
    }
    pub fn ordered(&self) -> Vec<&ModuleSetup> {
        self.order.iter().map(|i| &self.modules[*i]).collect()
    }
    pub fn owner_of_table(&self, name: &TableName) -> Option<&ModuleSetup> {
        self.owner(ObjectKind::Table, name.as_str())
    }
    pub fn owner_of_function(&self, name: &FunctionName) -> Option<&ModuleSetup> {
        self.owner(ObjectKind::Function, name.as_str())
    }
    pub fn owner_of_analyzer(&self, name: &AnalyzerName) -> Option<&ModuleSetup> {
        self.owner(ObjectKind::Analyzer, name.as_str())
    }
    pub(crate) fn owner(&self, kind: ObjectKind, name: &str) -> Option<&ModuleSetup> {
        self.modules.iter().find(|module| {
            module
                .ownership()
                .iter()
                .any(|claim| claim.matches(kind, name))
        })
    }
    pub fn depends_on(&self, module: &ModuleName, required: &ModuleName) -> bool {
        let Some(module) = self.module(module) else {
            return false;
        };
        module
            .requires()
            .iter()
            .any(|r| r.module() == required || self.depends_on(r.module(), required))
    }
    /// Enable requested optional modules and their prerequisites; kernels always execute.
    pub fn select(
        &self,
        enabled: Vec<ModuleName>,
    ) -> Result<ModuleSelection<'_>, DeclarationError> {
        let mut selected = BTreeSet::new();
        for module in &self.modules {
            if module.layer() == ModuleLayer::Kernel {
                self.enable(module.name(), &mut selected)?;
            }
        }
        for name in enabled {
            self.enable(&name, &mut selected)?;
        }
        Ok(ModuleSelection {
            registry: self,
            ordered: self
                .order
                .iter()
                .filter(|i| selected.contains(self.modules[**i].name()))
                .map(|i| &self.modules[*i])
                .collect(),
        })
    }
    fn enable(
        &self,
        name: &ModuleName,
        selected: &mut BTreeSet<ModuleName>,
    ) -> Result<(), DeclarationError> {
        let module = self
            .module(name)
            .ok_or_else(|| DeclarationError::new(format!("unknown selected module {name}")))?;
        if selected.insert(name.clone()) {
            for requirement in module.requires() {
                self.enable(requirement.module(), selected)?;
            }
        }
        Ok(())
    }
}
fn check_minimum(
    requirement: &LaneRequirement,
    required: &ModuleSetup,
) -> Result<(), DeclarationError> {
    if let Some(minimum) = requirement.minimum()
        && required
            .lane()
            .latest()
            .is_none_or(|latest| latest < minimum)
    {
        return Err(DeclarationError::new(format!(
            "module {} lacks required migration {}",
            required.name(),
            minimum
        )));
    }
    Ok(())
}
#[derive(Debug)]
pub struct ModuleSelection<'a> {
    registry: &'a ModuleRegistry,
    ordered: Vec<&'a ModuleSetup>,
}
impl<'a> ModuleSelection<'a> {
    pub fn registry(&self) -> &'a ModuleRegistry {
        self.registry
    }
    pub fn ordered(&self) -> &[&'a ModuleSetup] {
        &self.ordered
    }
    pub fn contains(&self, name: &ModuleName) -> bool {
        self.ordered.iter().any(|module| module.name() == name)
    }
}
